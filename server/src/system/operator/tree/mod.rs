//! Stable namespace identities, revisions and publication ownership. Entry IDs
//! never reuse a tombstone. Root is structural and never aliases entry zero.
use crate::system::operator::authority::{Authority, AuthorityError, GrantKey, GrantSubmission};
use alloc::{string::String, vec::Vec};
use kernel_api::model::identity::PendId;
use system_client::identity::TaskQuery;
use system_protocol::{
    EntryId, Mark, Name, Path, TaskId,
    operator::{self as api, Binding, Entry, EntryTarget, Fail, Kind, Permit},
    value::Rights,
};
pub mod gate;
mod imports;
pub(crate) mod journal;

struct Slot {
    name: Name,
    parent: EntryTarget,
    revision: u64,
    authority: GrantKey,
    node: Node,
}
enum Node {
    Pane(Vec<EntryId>),
    Tile {
        resource: imports::Key,
        id: u64,
        rights: Rights,
        permit: Permit,
        source: api::Source,
    },
}

pub struct Operator {
    actor: TaskId,
    roots: Vec<EntryId>,
    slots: Vec<Option<Slot>>,
    revision: u64,
    namespaces: Vec<api::Association>,
    imports: imports::Imports,
    pub(crate) authority: Authority,
    identity: Option<TaskQuery>,
    changes: Vec<api::Event>,
    sequence: u64,
    pub(crate) journal: journal::Journal,
}
impl Operator {
    pub fn new(controller: TaskId, actor: TaskId) -> Result<Self, Fail> {
        if controller == TaskId::NONE || actor == TaskId::NONE {
            return Err(Fail::Bad);
        }
        Ok(Self {
            actor,
            roots: Vec::new(),
            slots: Vec::new(),
            revision: 0,
            namespaces: Vec::new(),
            imports: imports::Imports::new()?,
            authority: Authority::new(controller, actor),
            identity: None,
            changes: Vec::new(),
            sequence: 0,
            journal: journal::Journal::default(),
        })
    }
    pub fn root(&self) -> Entry {
        Entry {
            operator: self.actor,
            target: EntryTarget::Root,
        }
    }
    pub fn identity(&self) -> Option<&TaskQuery> {
        self.identity.as_ref()
    }
    pub fn wire(&mut self, query: TaskQuery) -> Result<(), Fail> {
        if let Some(old) = &self.identity {
            old.finish().map_err(|_| Fail::Unjudged)?;
        }
        self.identity = Some(query);
        Ok(())
    }
    fn slot(&self, id: EntryId) -> Result<&Slot, Fail> {
        self.slots
            .get(usize::try_from(id.get()).map_err(|_| Fail::Unknown)?)
            .and_then(Option::as_ref)
            .filter(|slot| self.authority.live(slot.authority) && self.ancestors_live(&slot.parent))
            .ok_or(Fail::Unknown)
    }
    fn ancestors_live(&self, target: &EntryTarget) -> bool {
        let mut target = target;
        let mut remaining = self.slots.len();
        while let EntryTarget::Node(id) = target {
            if remaining == 0 {
                return false;
            }
            remaining -= 1;
            let Ok(slot) = self.raw_slot(*id) else {
                return false;
            };
            if !self.authority.live(slot.authority) {
                return false;
            }
            target = &slot.parent;
        }
        true
    }
    fn raw_slot(&self, id: EntryId) -> Result<&Slot, Fail> {
        self.slots
            .get(usize::try_from(id.get()).map_err(|_| Fail::Unknown)?)
            .and_then(Option::as_ref)
            .ok_or(Fail::Unknown)
    }
    fn target(&self, entry: &Entry) -> Result<EntryTarget, Fail> {
        if entry.operator != self.actor {
            return Err(Fail::Denied);
        }
        if let EntryTarget::Node(id) = entry.target {
            self.slot(id)?;
        }
        Ok(entry.target.clone())
    }
    fn children(&self, target: &EntryTarget) -> Result<&[EntryId], Fail> {
        match target {
            EntryTarget::Root => Ok(&self.roots),
            EntryTarget::Node(id) => match &self.slot(*id)?.node {
                Node::Pane(kids) => Ok(kids),
                Node::Tile { .. } => Err(Fail::NotAPane),
            },
        }
    }
    fn children_mut(&mut self, target: &EntryTarget) -> Result<&mut Vec<EntryId>, Fail> {
        match target {
            EntryTarget::Root => Ok(&mut self.roots),
            EntryTarget::Node(id) => {
                let slot = self
                    .slots
                    .get_mut(usize::try_from(id.get()).map_err(|_| Fail::Unknown)?)
                    .and_then(Option::as_mut)
                    .ok_or(Fail::Unknown)?;
                match &mut slot.node {
                    Node::Pane(kids) => Ok(kids),
                    _ => Err(Fail::NotAPane),
                }
            }
        }
    }
    pub fn child(&self, parent: &Entry, name: &Name) -> Result<Option<Entry>, Fail> {
        let target = self.target(parent)?;
        Ok(self
            .children(&target)?
            .iter()
            .find(|&&id| self.slot(id).is_ok_and(|slot| slot.name == *name))
            .map(|&id| Entry {
                operator: self.actor,
                target: EntryTarget::Node(id),
            }))
    }
    pub fn binding(&self, entry: &Entry) -> Result<Binding, Fail> {
        let EntryTarget::Node(id) = self.target(entry)? else {
            return Ok(Binding {
                entry: self.root(),
                revision: 0,
            });
        };
        Ok(Binding {
            entry: entry.clone(),
            revision: self.slot(id)?.revision,
        })
    }
    fn check(&self, entry: &Entry, expected: u64) -> Result<EntryId, Fail> {
        let current = self.binding(entry)?;
        if current.revision != expected {
            return Err(Fail::Conflict);
        }
        match current.entry.target {
            EntryTarget::Node(id) => Ok(id),
            EntryTarget::Root => Err(Fail::Denied),
        }
    }
    pub fn seek(&self, base: &Entry, path: &Path) -> Result<Entry, Fail> {
        let mut entry = base.clone();
        self.target(&entry)?;
        for component in path.as_str().split('/').filter(|v| !v.is_empty()) {
            entry = self
                .child(&entry, &Name::new(component).ok_or(Fail::Bad)?)?
                .ok_or(Fail::Unknown)?;
        }
        Ok(entry)
    }
    pub fn path(&self, entry: &Entry) -> Result<Path, Fail> {
        let mut target = self.target(entry)?;
        let mut parts: [&str; 8] = [""; 8];
        let mut count = 0;
        while let EntryTarget::Node(id) = target {
            if count == parts.len() {
                return Err(Fail::Full);
            }
            let slot = self.slot(id)?;
            parts[count] = slot.name.as_str();
            count += 1;
            target = slot.parent.clone();
        }
        let mut text = String::new();
        text.try_reserve(255).map_err(|_| Fail::Full)?;
        for part in parts[..count].iter().rev() {
            if !text.is_empty() {
                text.push('/');
            }
            text.push_str(part);
        }
        Path::new(&text).ok_or(Fail::Full)
    }
    pub fn relative(&self, base: &Entry, target: &Entry) -> Result<Path, Fail> {
        let base = self.path(base)?;
        let target = self.path(target)?;
        let relative = if base.as_str().is_empty() {
            target.as_str()
        } else if base == target {
            ""
        } else {
            target
                .as_str()
                .strip_prefix(base.as_str())
                .and_then(|tail| tail.strip_prefix('/'))
                .ok_or(Fail::Denied)?
        };
        Path::new(relative).ok_or(Fail::Full)
    }
    pub fn list(&self, entry: &Entry) -> Result<api::Listing, Fail> {
        let target = self.target(entry)?;
        Ok(api::Listing {
            ids: self
                .children(&target)?
                .iter()
                .copied()
                .filter(|&id| self.slot(id).is_ok())
                .collect(),
        })
    }
    pub fn name(&self, entry: &Entry) -> Result<Name, Fail> {
        match self.target(entry)? {
            EntryTarget::Root => Err(Fail::Unknown),
            EntryTarget::Node(id) => Ok(self.slot(id)?.name.clone()),
        }
    }
    pub fn permit(&self, entry: &Entry) -> Result<Permit, Fail> {
        match self.target(entry)? {
            EntryTarget::Root => Ok(Permit::Public),
            EntryTarget::Node(id) => match &self.slot(id)?.node {
                Node::Pane(_) => Ok(Permit::Public),
                Node::Tile { permit, .. } => Ok(permit.clone()),
            },
        }
    }
    pub fn opener(&self, id: EntryId) -> Result<TaskId, Fail> {
        match &self.slot(id)?.node {
            Node::Pane(_) => Err(Fail::NotATile),
            Node::Tile { resource, .. } => self.imports.source(*resource),
        }
    }
    fn prepare_change(&mut self) -> Result<u64, Fail> {
        self.changes.try_reserve(1).map_err(|_| Fail::Full)?;
        self.sequence.checked_add(1).ok_or(Fail::Full)?;
        self.revision.checked_add(1).ok_or(Fail::Full)
    }
    fn record(&mut self, kind: Kind, entry: &Entry, path: Path, owner: TaskId, revision: u64) {
        self.revision = revision;
        self.sequence += 1;
        let EntryTarget::Node(id) = entry.target else {
            unreachable!()
        };
        self.changes.push(api::Event {
            sequence: self.sequence,
            kind: weave::Coded(kind),
            path,
            entry: id,
            owner,
        });
    }
    fn destination(&self, parent: &Entry, name: &Name) -> Result<(EntryTarget, Path), Fail> {
        let target = self.target(parent)?;
        self.children(&target)?;
        let base = self.path(parent)?;
        let path = if base.as_str().is_empty() {
            Path::new(name.as_str())
        } else {
            Path::new(&alloc::format!("{}/{}", base.as_str(), name.as_str()))
        }
        .ok_or(Fail::Full)?;
        Ok((target, path))
    }
    fn insert(
        &mut self,
        name: Name,
        parent: EntryTarget,
        node: Node,
        authority: GrantKey,
        revision: u64,
    ) -> Result<Entry, Fail> {
        if self.children(&parent)?.len() >= api::PANE_CAP {
            return Err(Fail::Full);
        }
        self.slots.try_reserve(1).map_err(|_| Fail::Full)?;
        self.children_mut(&parent)?
            .try_reserve(1)
            .map_err(|_| Fail::Full)?;
        let id = EntryId::new(u64::try_from(self.slots.len()).map_err(|_| Fail::Full)?);
        self.slots.push(Some(Slot {
            name,
            parent: parent.clone(),
            revision,
            authority,
            node,
        }));
        self.children_mut(&parent)?.push(id);
        Ok(Entry {
            operator: self.actor,
            target: EntryTarget::Node(id),
        })
    }
    pub fn part(&mut self, context: Context, request: &api::Part) -> Result<Binding, Fail> {
        self.authorized(
            context,
            &request.change.entry,
            api::Ops::PART,
            Some(&request.name),
        )?;
        let authority = context.authority;
        if request.change.expected != 0 || request.permit != Permit::Public {
            return Err(Fail::Bad);
        }
        if let Some(entry) = self.child(&request.change.entry, &request.name)? {
            self.children(&entry.target)?;
            return self.binding(&entry);
        }
        let (target, path) = self.destination(&request.change.entry, &request.name)?;
        let revision = self.prepare_change()?;
        let entry = self.insert(
            request.name.clone(),
            target,
            Node::Pane(Vec::new()),
            authority,
            revision,
        )?;
        self.record(Kind::Parted, &entry, path, TaskId::NONE, revision);
        self.binding(&entry)
    }
    pub fn bind(
        &mut self,
        context: Context,
        request: &api::Bind,
        replace: bool,
    ) -> Result<Binding, Fail> {
        if !self.authority.live(context.authority) {
            return Err(Fail::Denied);
        }
        let token = request.resource.try_into().map_err(|_| Fail::Bad)?;
        let resource = self.imports.admit(
            token,
            context.caller,
            context.authority,
            request.source.clone(),
            request.rights,
        )?;
        let result = self
            .authorized(
                context,
                &request.change.entry,
                if replace {
                    api::Ops::REPLACE
                } else {
                    api::Ops::BIND
                },
                if replace { None } else { Some(&request.name) },
            )
            .and_then(|()| self.restrictions(context, request))
            .and_then(|()| self.commit_binding(context.authority, request, resource, replace));
        if result.is_err() {
            self.imports.reject(resource);
        }
        result
    }
    fn commit_binding(
        &mut self,
        authority: GrantKey,
        request: &api::Bind,
        resource: imports::Key,
        replace: bool,
    ) -> Result<Binding, Fail> {
        let tile = Node::Tile {
            resource,
            id: request.resource_id,
            rights: request.rights,
            permit: request.permit.clone(),
            source: request.source.clone(),
        };
        if replace {
            let id = self.check(&request.change.entry, request.change.expected)?;
            let old = self.slot(id)?;
            let Node::Tile {
                resource: old_resource,
                ..
            } = old.node
            else {
                return Err(Fail::NotATile);
            };
            if old.name != request.name {
                return Err(Fail::Conflict);
            }
            let path = self.path(&request.change.entry)?;
            let revision = self.prepare_change()?;
            let slot = self.slots[id.get() as usize].as_mut().unwrap();
            slot.node = tile;
            slot.authority = authority;
            slot.revision = revision;
            self.imports.retain(old_resource);
            self.record(
                Kind::Rebound,
                &request.change.entry,
                path,
                request.source.task,
                revision,
            );
            return self.binding(&request.change.entry);
        }
        if request.change.expected != 0 {
            return Err(Fail::Bad);
        }
        if let Some(entry) = self.child(&request.change.entry, &request.name)? {
            let slot = self.slot(match entry.target {
                EntryTarget::Node(id) => id,
                _ => unreachable!(),
            })?;
            if matches!(&slot.node, Node::Tile { id, rights, permit, source, .. } if *id == request.resource_id && *rights == request.rights && *permit == request.permit && *source == request.source)
                && slot.authority == authority
            {
                self.imports.reject(resource);
                return self.binding(&entry);
            }
            return Err(Fail::Conflict);
        }
        let (parent, path) = self.destination(&request.change.entry, &request.name)?;
        let revision = self.prepare_change()?;
        let entry = self.insert(request.name.clone(), parent, tile, authority, revision)?;
        self.record(Kind::Landed, &entry, path, request.source.task, revision);
        self.binding(&entry)
    }
    pub fn remove(
        &mut self,
        context: Context,
        request: &api::Mutation,
        pane: bool,
    ) -> Result<Binding, Fail> {
        self.authorized(
            context,
            &request.entry,
            if pane {
                api::Ops::UNPART
            } else {
                api::Ops::UNBIND
            },
            None,
        )?;
        let id = self.check(&request.entry, request.expected)?;
        self.remove_node(id, pane, false)
    }
    fn remove_node(&mut self, id: EntryId, pane: bool, revoked: bool) -> Result<Binding, Fail> {
        let slot = self.raw_slot(id)?;
        let resource = match &slot.node {
            Node::Pane(_) if !pane => return Err(Fail::NotATile),
            Node::Pane(kids) if !kids.is_empty() => return Err(Fail::NonEmpty),
            Node::Tile { .. } if pane => return Err(Fail::NotAPane),
            Node::Tile { resource, .. } => Some(*resource),
            Node::Pane(_) => None,
        };
        let entry = Entry {
            operator: self.actor,
            target: EntryTarget::Node(id),
        };
        // An ancestor's logical revocation must not erase the retained path.
        let path = self.raw_path(id)?;
        let old_revision = slot.revision;
        let parent = slot.parent.clone();
        let revision = self.prepare_change()?;
        let kids = match parent {
            EntryTarget::Root => &mut self.roots,
            EntryTarget::Node(parent) => match &mut self.slots[parent.get() as usize]
                .as_mut()
                .ok_or(Fail::Unknown)?
                .node
            {
                Node::Pane(kids) => kids,
                _ => return Err(Fail::NotAPane),
            },
        };
        kids.retain(|&kid| kid != id);
        self.slots[id.get() as usize] = None;
        if let Some(resource) = resource {
            if revoked {
                self.imports.reject(resource);
            } else {
                self.imports.retain(resource);
            }
        }
        self.record(Kind::Trimmed, &entry, path, TaskId::NONE, revision);
        Ok(Binding {
            entry,
            revision: old_revision,
        })
    }
    fn raw_path(&self, id: EntryId) -> Result<Path, Fail> {
        let mut names: [&str; 8] = [""; 8];
        let mut count = 0;
        let mut target = EntryTarget::Node(id);
        while let EntryTarget::Node(id) = target {
            if count == 8 {
                return Err(Fail::Full);
            }
            let slot = self.raw_slot(id)?;
            names[count] = slot.name.as_str();
            count += 1;
            target = slot.parent.clone();
        }
        Path::new(
            &names[..count]
                .iter()
                .rev()
                .copied()
                .collect::<Vec<_>>()
                .join("/"),
        )
        .ok_or(Fail::Full)
    }
    pub fn read(&mut self, entry: &Entry, caller: TaskId) -> Result<api::Resource, Fail> {
        let binding = self.binding(entry)?;
        let EntryTarget::Node(id) = entry.target else {
            return Err(Fail::NotATile);
        };
        let (resource, rights, permit) = match &self.slot(id)?.node {
            Node::Pane(_) => return Err(Fail::NotATile),
            Node::Tile {
                resource,
                rights,
                permit,
                ..
            } => (*resource, *rights, permit.clone()),
        };
        match self.imports.send(resource, caller) {
            Ok(token) => Ok(api::Resource {
                binding,
                token,
                rights,
                permit,
            }),
            Err(Fail::Dead) => {
                self.remove_node(id, false, false)?;
                Err(Fail::Dead)
            }
            Err(error) => Err(error),
        }
    }
    pub fn associate(&mut self, context: Context, request: api::Association) -> Result<(), Fail> {
        if !matches!(
            self.authority.grant_at(context.via, context.caller),
            Some((GrantKey::Root, None))
        ) {
            return Err(Fail::Denied);
        }
        if request.mark.get() != kernel_api::model::identity::Mark::of(request.name.as_str()).get()
        {
            return Err(Fail::Denied);
        }
        self.children(&self.target(&request.entry)?)?;
        if let Some(old) = self.namespaces.iter().find(|old| old.mark == request.mark) {
            return if old == &request {
                Ok(())
            } else {
                Err(Fail::Conflict)
            };
        }
        if self.namespaces.len() >= 256 {
            return Err(Fail::Full);
        }
        self.namespaces.try_reserve(1).map_err(|_| Fail::Full)?;
        self.namespaces.push(request);
        Ok(())
    }
    pub fn resolve(&self, mark: Mark, name: &Name) -> Result<Entry, Fail> {
        if mark.get() != kernel_api::model::identity::Mark::of(name.as_str()).get() {
            return Err(Fail::Denied);
        }
        let found = self
            .namespaces
            .iter()
            .find(|old| old.mark == mark)
            .ok_or(Fail::Unknown)?;
        if found.name != *name {
            return Err(Fail::Conflict);
        }
        self.children(&self.target(&found.entry)?)?;
        Ok(found.entry.clone())
    }
    pub fn grant(
        &mut self,
        request: GrantSubmission,
    ) -> Result<(api::GrantState, GrantKey), AuthorityError> {
        let (_, parent) = self
            .authority
            .grant_at(request.via, request.caller)
            .ok_or(AuthorityError::Denied)?;
        let base = parent
            .map(|spec| spec.anchor.clone())
            .unwrap_or_else(|| self.root());
        let relative = self
            .relative(&base, &request.spec.anchor)
            .map_err(|_| AuthorityError::Denied)?;
        self.authority.grant(request, &relative)
    }
    pub fn authorized(
        &self,
        context: Context,
        entry: &Entry,
        op: api::Ops,
        name: Option<&Name>,
    ) -> Result<(), Fail> {
        let (key, spec) = self
            .authority
            .grant_at(context.via, context.caller)
            .ok_or(Fail::Denied)?;
        if key != context.authority {
            return Err(Fail::Denied);
        }
        let Some(spec) = spec else {
            self.target(entry)?;
            return Ok(());
        };
        if !spec.ops.contains(op) {
            return Err(Fail::Denied);
        }
        let mut relative = self.relative(&spec.anchor, entry)?;
        if let Some(name) = name {
            relative = Path::new(&if relative.as_str().is_empty() {
                String::from(name.as_str())
            } else {
                alloc::format!("{}/{}", relative.as_str(), name.as_str())
            })
            .ok_or(Fail::Full)?;
        }
        if !spec.scope.allows(relative.as_str()) {
            return Err(Fail::Denied);
        }
        Ok(())
    }
    pub(crate) fn permission(&self, context: Context, op: api::Ops) -> Result<(), Fail> {
        let (key, spec) = self
            .authority
            .grant_at(context.via, context.caller)
            .ok_or(Fail::Denied)?;
        if key != context.authority || spec.is_some_and(|spec| !spec.ops.contains(op)) {
            return Err(Fail::Denied);
        }
        Ok(())
    }
    pub fn restrictions(&self, context: Context, request: &api::Bind) -> Result<(), Fail> {
        let (_, spec) = self
            .authority
            .grant_at(context.via, context.caller)
            .ok_or(Fail::Denied)?;
        if let Some(spec) = spec
            && (spec
                .permit
                .0
                .as_ref()
                .is_some_and(|permit| permit != &request.permit)
                || spec
                    .source
                    .0
                    .is_some_and(|task| task != request.source.task)
                || spec
                    .source_mark
                    .0
                    .is_some_and(|mark| mark != request.source.mark))
        {
            return Err(Fail::Denied);
        }
        Ok(())
    }
    pub fn withdraw(&mut self, key: GrantKey) -> Result<(), Fail> {
        self.imports.withdraw(key);
        for index in (0..self.slots.len()).rev() {
            if self.slots[index]
                .as_ref()
                .is_some_and(|slot| slot.authority == key)
            {
                let pane = matches!(self.slots[index].as_ref().unwrap().node, Node::Pane(_));
                self.remove_node(EntryId::new(index as u64), pane, true)?;
            }
        }
        self.imports.sweep();
        if self.imports.settled(key) {
            Ok(())
        } else {
            Err(Fail::Unjudged)
        }
    }
    pub fn changes(&mut self) -> Vec<api::Event> {
        core::mem::take(&mut self.changes)
    }
    pub(crate) fn blocked(&self) -> bool {
        self.imports.blocked()
    }
    pub(crate) fn contains(&self, token: PendId) -> bool {
        self.imports.contains(token)
    }
    pub(crate) fn discard(&mut self, context: Context, request: &api::Bind) -> Result<(), Fail> {
        let token = request.resource.try_into().map_err(|_| Fail::Bad)?;
        if self.imports.contains(token) {
            return Ok(());
        }
        let key = self.imports.admit(
            token,
            context.caller,
            context.authority,
            request.source.clone(),
            request.rights,
        )?;
        self.imports.reject(key);
        Ok(())
    }
    pub fn sweep(&mut self) -> bool {
        self.imports.sweep()
    }
    pub fn finish(&mut self) -> Result<(), Fail> {
        if let Some(query) = &self.identity {
            query.finish().map_err(|_| Fail::Unjudged)?;
        }
        self.imports.finish()
    }
}
#[derive(Clone, Copy)]
pub struct Context {
    pub caller: TaskId,
    pub via: PendId,
    pub authority: GrantKey,
}

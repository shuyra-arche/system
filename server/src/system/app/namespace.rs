//! System's namespace plans. Operator remains the authority for every mutation.
use super::services::{Services, give, inspect, release};
use crate::system::declaration::{self, Namespace};
use alloc::{string::String, vec::Vec};
use kernel_api::model::{
    identity::{Mark, PendId, TaskId},
    pend::Distributable,
};
use system_protocol::{
    Name, Path, operator as op,
    value::{Optional, Outcome, Rights},
};
use unit_contract::document::Placement;

pub(crate) struct Layout {
    next: u64,
    keys: Vec<(String, op::Entry)>,
}
impl Layout {
    pub fn new() -> Self {
        Self {
            next: 1,
            keys: Vec::new(),
        }
    }
    fn number(&mut self) -> u64 {
        let id = self.next;
        self.next = id
            .checked_add(1)
            .unwrap_or_else(|| super::services::fatal("system: namespace sequence exhausted"));
        id
    }
    pub fn root(&self, services: &Services) -> op::Entry {
        op::Entry {
            operator: services.operator().into(),
            target: op::EntryTarget::Root,
        }
    }
    pub fn part(
        &mut self,
        services: &Services,
        parent: op::Entry,
        name: &str,
    ) -> Result<op::Binding, op::Fail> {
        let request = op::Part {
            change: op::Mutation {
                id: op::OperationId::new(self.number()).unwrap(),
                entry: parent,
                expected: 0,
            },
            name: Name::new(name).ok_or(op::Fail::Bad)?,
            permit: op::Permit::Public,
        };
        result(services.mutate::<op::authority::operations::Part>(request))
    }
    pub fn key(&mut self, services: &Services, name: &str) -> Result<op::Entry, op::Fail> {
        if let Some((_, entry)) = self.keys.iter().find(|(key, _)| key == name) {
            return Ok(entry.clone());
        }
        self.keys.try_reserve(1).map_err(|_| op::Fail::Full)?;
        let entry = self.part(services, self.root(services), name)?.entry;
        result(
            services.mutate::<op::authority::operations::Associate>(op::Association {
                mark: Mark::of(name).into(),
                name: Name::new(name).ok_or(op::Fail::Bad)?,
                entry: entry.clone(),
            }),
        )?;
        let resolved = result(services.query::<op::operations::Resolve>(op::Resolve {
            mark: Mark::of(name).into(),
            name: Name::new(name).ok_or(op::Fail::Bad)?,
        }))?;
        if resolved != entry {
            return Err(op::Fail::Conflict);
        }
        self.keys.push((String::from(name), entry.clone()));
        Ok(entry)
    }
    pub fn path(
        &mut self,
        services: &Services,
        mut parent: op::Entry,
        path: &Path,
    ) -> Result<op::Entry, op::Fail> {
        if !path.as_str().is_empty() {
            for name in path.as_str().split('/') {
                parent = self.part(services, parent, name)?.entry;
            }
        }
        Ok(parent)
    }
    pub fn resource(
        &self,
        services: &Services,
        base: op::Entry,
        name: &str,
    ) -> Result<op::Resource, op::Fail> {
        let binding = result(
            services.mutate::<op::authority::operations::Find>(op::Query {
                anchor: base,
                name: Name::new(name).ok_or(op::Fail::Bad)?,
            }),
        )?;
        result(services.mutate::<op::authority::operations::Read>(op::At {
            entry: binding.entry,
        }))
    }
}

/// All acquired grants, publications and private delegation roots remain here,
/// including when a later setup step fails. Entry coordinates are observations.
#[derive(Default)]
pub(crate) struct Installation {
    pub anchors: Vec<op::Entry>,
    grants: Vec<op::GrantId>,
    bindings: Vec<op::Binding>,
    containers: Vec<op::Binding>,
    roots: Vec<PendId>,
    removal: Option<op::OperationId>,
}
impl Installation {
    pub fn install(
        &mut self,
        layout: &mut Layout,
        services: &Services,
        instance: u64,
        task: TaskId,
        unit: &declaration::Unit,
        source: impl Fn(Mark, kernel_api::model::pend::Limits) -> Option<PendId>,
    ) -> Result<(), op::Fail> {
        self.anchors
            .try_reserve_exact(unit.namespaces().len())
            .map_err(|_| op::Fail::Full)?;
        self.grants
            .try_reserve_exact(unit.namespaces().len())
            .map_err(|_| op::Fail::Full)?;
        self.containers
            .try_reserve_exact(unit.namespaces().len())
            .map_err(|_| op::Fail::Full)?;
        let publications: usize = unit
            .namespaces()
            .iter()
            .map(|ns| ns.publications.len())
            .sum();
        self.bindings
            .try_reserve_exact(publications)
            .map_err(|_| op::Fail::Full)?;
        self.roots
            .try_reserve_exact(publications)
            .map_err(|_| op::Fail::Full)?;
        for ns in unit.namespaces() {
            let mut anchor = layout.key(services, ns.key)?;
            if ns.placement == Placement::Instance {
                let container = layout.part(services, anchor, &alloc::format!("i{instance}"))?;
                anchor = container.entry.clone();
                self.containers.push(container);
            }
            if ns.placement == Placement::Instance && !ns.path.as_str().is_empty() {
                self.containers
                    .try_reserve(ns.path.as_str().split('/').count())
                    .map_err(|_| op::Fail::Full)?;
                for name in ns.path.as_str().split('/') {
                    let child = layout.part(services, anchor, name)?;
                    anchor = child.entry.clone();
                    self.containers.push(child);
                }
            } else {
                anchor = layout.path(services, anchor, &ns.path)?;
            }
            self.publish(layout, services, anchor.clone(), ns, &source)?;
            let id = op::GrantId::new(layout.number()).unwrap();
            // The pending grant's identity belongs to this installation before
            // the effect. An indeterminate reply terminates System with owners.
            self.grants.push(id);
            let request = op::GrantSpec {
                id,
                parent: Optional(None),
                holder: task.into(),
                anchor: anchor.clone(),
                scope: ns.scope.clone(),
                ops: ns.operations,
                permit: Optional(ns.permit.clone()),
                source: Optional(Some(task.into())),
                source_mark: Optional(ns.source_mark.map(Into::into)),
            };
            let deadline =
                kernel_client::call::room::clock().saturating_add(super::services::LIMIT_NS);
            loop {
                let granted =
                    result(services.mutate::<op::authority::operations::Grant>(request.clone()))?;
                match granted.state.0 {
                    op::GrantState::Granted => break,
                    op::GrantState::Pending => {
                        super::services::bound(deadline, "system: namespace grant timeout")
                    }
                    _ => return Err(op::Fail::Denied),
                }
            }
            self.anchors.push(anchor);
        }
        Ok(())
    }
    fn publish(
        &mut self,
        layout: &mut Layout,
        services: &Services,
        anchor: op::Entry,
        ns: &Namespace,
        source: &impl Fn(Mark, kernel_api::model::pend::Limits) -> Option<PendId>,
    ) -> Result<(), op::Fail> {
        for publication in &ns.publications {
            let original = source(publication.mark, publication.limits).ok_or(op::Fail::Unknown)?;
            let meta = inspect(original);
            let mut limits = publication.limits;
            limits.distributable = Distributable::Yes;
            let mut request = kernel_api::model::pend::AccordRequest {
                src: original,
                target: kernel_api::model::pend::AccordTarget::Caller,
                mark: publication.mark,
                limits,
            }
            .to_bytes();
            let root = kernel_client::call::pend::accord(kernel_api::address::VirtAddr::new(
                request.as_mut_ptr().addr(),
            ))
            .map_err(|_| op::Fail::Denied)?;
            self.roots.push(root);
            let delivered = give(root, services.operator(), publication.mark, limits)
                .map_err(|_| op::Fail::Denied)?;
            let binding = result(
                services.mutate::<op::authority::operations::Bind>(op::Bind {
                    change: op::Mutation {
                        id: op::OperationId::new(layout.number()).unwrap(),
                        entry: anchor.clone(),
                        expected: 0,
                    },
                    name: publication.name.clone(),
                    resource: delivered.into(),
                    resource_id: meta.target.words()[1] as u64,
                    rights: Rights(publication.limits),
                    permit: publication.permit.clone(),
                    source: op::Source {
                        task: meta.owner.into(),
                        mark: publication.mark.into(),
                    },
                }),
            )?;
            self.bindings.push(binding);
        }
        Ok(())
    }
    pub fn finish(&mut self, layout: &mut Layout, services: &Services) -> Result<bool, op::Fail> {
        if let Some(id) = self.grants.last().copied() {
            let status = result(services.mutate::<op::authority::operations::Revoke>(id))?;
            if status.state.0 == op::GrantState::Closed {
                self.grants.pop();
            }
            return Ok(false);
        }
        if let Some(binding) = self.bindings.last() {
            let id = *self
                .removal
                .get_or_insert_with(|| op::OperationId::new(layout.number()).unwrap());
            let status = result(services.mutate::<op::authority::operations::Unbind>(
                op::Mutation {
                    id,
                    entry: binding.entry.clone(),
                    expected: binding.revision,
                },
            ))?;
            if status.state.0 == op::OperationState::Applied {
                self.bindings.pop();
                self.removal = None;
            }
            return Ok(false);
        }
        if let Some(root) = self.roots.last().copied() {
            if release(root) {
                self.roots.pop();
            }
            return Ok(false);
        }
        if let Some(binding) = self.containers.last() {
            let id = *self
                .removal
                .get_or_insert_with(|| op::OperationId::new(layout.number()).unwrap());
            let status = result(services.mutate::<op::authority::operations::Unpart>(
                op::Mutation {
                    id,
                    entry: binding.entry.clone(),
                    expected: binding.revision,
                },
            ))?;
            if status.state.0 == op::OperationState::Applied {
                self.containers.pop();
                self.removal = None;
            }
            return Ok(false);
        }
        self.anchors.clear();
        Ok(true)
    }
}
pub(crate) fn result<T, E>(value: Outcome<T, E>) -> Result<T, E> {
    match value {
        Outcome::Success(value) => Ok(value),
        Outcome::Failure(error) => Err(error),
    }
}

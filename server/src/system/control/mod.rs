//! Program intent, instance ownership and construction receipts have distinct IDs.
//! All native effects run through the instance's retained Job/Installation.
mod construction;
pub(crate) mod identity;
pub(crate) mod instance;
use self::{
    identity::Roster,
    instance::{Instance, InstanceId, Intent},
};
use crate::{
    boot::Catalog as Images,
    system::{
        app::{
            namespace::{Layout, result},
            services::{Services, give, lookup},
        },
        declaration::{Catalog, Unit},
        loader::{Image, Loader, ProgramKind, Spawn},
        supply::Registry,
    },
};
use alloc::{string::String, sync::Arc, vec::Vec};
use kernel_api::model::{
    identity::{Mark, PendId, TaskId},
    modifiable::Modifiable,
    pend::{CloneRights, CreationRights, Distributable, Limits, Operations, PendScope},
    unit::{ControlId, UnitState},
};
use system_protocol::{
    control as api, identity as id, loader as build,
    value::{Optional, Outcome},
};
use unit_contract::document as d;
pub const E_PROGRAM: usize = 3;
const CAPACITY: usize = 256;
struct Program {
    declaration: Arc<Unit>,
    current: Option<InstanceId>,
    run_requested: bool,
}
struct Creation {
    caller: TaskId,
    request: build::Build,
    instance: InstanceId,
    resolve: Option<system_protocol::PendId>,
    owner_resolve: Option<system_protocol::PendId>,
    claimed: bool,
    expires: u64,
}

pub(crate) struct Control {
    catalog: Catalog,
    images: Images<'static>,
    programs: Vec<Program>,
    instances: Vec<Instance>,
    creates: Vec<Creation>,
    next: u64,
    pub loader: Loader,
    pub roster: Roster,
    pub layout: Layout,
    startup: Vec<Arc<Unit>>,
    cursor: usize,
    closing: bool,
    guard: Option<ControlId>,
}
impl Control {
    pub fn new(
        catalog: Catalog,
        images: Images<'static>,
        loader: Loader,
        guard: Option<ControlId>,
    ) -> Result<Self, &'static str> {
        let startup = catalog.startup().map_err(|_| "startup policy")?;
        let programs = catalog
            .units()
            .iter()
            .filter(|unit| unit.name() != catalog.system())
            .map(|declaration| Program {
                declaration: declaration.clone(),
                current: None,
                run_requested: false,
            })
            .collect();
        let mut instances = Vec::new();
        instances
            .try_reserve_exact(CAPACITY)
            .map_err(|_| "instance capacity")?;
        let mut creates = Vec::new();
        creates
            .try_reserve_exact(CAPACITY)
            .map_err(|_| "construction capacity")?;
        Ok(Self {
            catalog,
            images,
            programs,
            instances,
            creates,
            next: 1,
            loader,
            roster: Roster::default(),
            layout: Layout::new(),
            startup,
            cursor: 0,
            closing: false,
            guard,
        })
    }
    fn instance(&self, id: InstanceId) -> &Instance {
        self.instances
            .iter()
            .find(|instance| instance.id == id)
            .expect("owned instance identity")
    }
    fn slot(&self, name: &str) -> Option<usize> {
        self.programs
            .iter()
            .position(|program| program.declaration.name() == name)
    }
    fn caller_unit(&self, caller: TaskId) -> Option<Arc<Unit>> {
        self.instances
            .iter()
            .find(|instance| instance.ready() && instance.contains_caller(caller))
            .and_then(|instance| instance.program.clone())
    }
    fn spawn(
        &mut self,
        owner: TaskId,
        program: Option<Arc<Unit>>,
        bytes: &[u8],
        args: &[usize],
        stack: usize,
        identity: id::Install,
    ) -> Result<InstanceId, api::Fail> {
        if self.closing {
            return Err(api::Fail::NotReady);
        }
        if self.instances.len() == CAPACITY {
            return Err(api::Fail::Full);
        }
        let id = InstanceId::new(self.next).ok_or(api::Fail::Full)?;
        let next = self.next.checked_add(1).ok_or(api::Fail::Full)?;
        let kind =
            program
                .as_ref()
                .map_or(ProgramKind::User, |program| match program.loader().kind {
                    crate::system::declaration::Space::User => ProgramKind::User,
                    crate::system::declaration::Space::Supervisor => ProgramKind::Supervisor,
                });
        let guard = if program.as_ref().is_some_and(|unit| unit.control().guarded) {
            self.guard.ok_or(api::Fail::Denied)?
        } else {
            ControlId::NONE
        };
        let job = self
            .loader
            .prepare(
                Image { bytes, kind },
                Spawn {
                    args,
                    stack,
                    guard,
                    member_exit: true,
                    creation: CreationRights::FLAT
                        | CreationRights::SIGN
                        | CreationRights::CLONE
                        | CreationRights::SHARE,
                },
            )
            .map_err(|_| api::Fail::BadImage)?;
        self.next = next;
        self.instances
            .push(Instance::new(id, owner, program, job, identity, false));
        Ok(id)
    }
    fn mint(
        &mut self,
        index: usize,
        services: &Services,
        start: bool,
    ) -> Result<InstanceId, api::Fail> {
        if let Some(id) = self.programs[index].current
            && !self.instance(id).retired()
        {
            return Ok(id);
        }
        let program = self.programs[index].declaration.clone();
        let image = self
            .images
            .find(program.loader().image)
            .ok_or(api::Fail::BadImage)?
            .bytes;
        let identity = id::Install::Derive(id::PrincipalId {
            authority: services.identity_task().into(),
            slot: 0,
        });
        let id = self.spawn(
            services.caller,
            Some(program.clone()),
            image,
            &program.loader().args,
            program.loader().stack,
            identity,
        )?;
        if start {
            self.instances.last_mut().unwrap().request(Intent::Run)?;
        }
        self.programs[index].current = Some(id);
        self.programs[index].run_requested = true;
        Ok(id)
    }
    pub fn progress(
        &mut self,
        services: &Services,
        sources: &Registry,
    ) -> Result<bool, &'static str> {
        let anchors: Vec<_> = self
            .programs
            .iter()
            .filter_map(|program| {
                let instance = self.instance(program.current?);
                let values: Vec<_> = (0..program.declaration.namespaces().len())
                    .filter_map(|index| instance.anchor(index))
                    .collect();
                Some((String::from(program.declaration.name()), values))
            })
            .collect();
        let now = kernel_client::call::room::clock();
        for record in &self.creates {
            if !record.claimed
                && (now >= record.expires
                    || lookup(record.caller).is_none_or(|info| info.state == UnitState::Exited))
            {
                let instance = self
                    .instances
                    .iter_mut()
                    .find(|instance| instance.id == record.instance)
                    .unwrap();
                if !instance.retired() {
                    let _ = instance.request(Intent::Stop);
                }
            }
        }
        let mut progress = false;
        for instance in &mut self.instances {
            progress |= instance.progress(
                &mut self.loader,
                services,
                &mut self.roster,
                &mut self.layout,
                sources,
                &anchors,
            )?;
        }
        self.loader
            .advance()
            .map_err(|_| "image cache retirement")?;
        self.close_round();
        if !self.closing && self.cursor < self.startup.len() {
            let unit = self.startup[self.cursor].clone();
            let index = self.slot(unit.name()).ok_or("startup program missing")?;
            if self.programs[index].current.is_none() {
                let ready = unit
                    .control()
                    .requires
                    .iter()
                    .chain(unit.control().after.iter())
                    .all(|name| {
                        *name == self.catalog.system()
                            || self.slot(name).is_none_or(|at| {
                                self.programs[at]
                                    .current
                                    .is_some_and(|id| self.instance(id).ready())
                            })
                    });
                if ready {
                    self.mint(index, services, true)
                        .map_err(|_| "startup construction admission")?;
                    progress = true;
                }
            } else if self.programs[index]
                .current
                .is_some_and(|id| self.instance(id).ready() || self.instance(id).retired())
            {
                if self.programs[index]
                    .current
                    .is_some_and(|id| self.instance(id).failure().is_some())
                {
                    return Err("startup instance failed");
                }
                self.cursor += 1;
                progress = true;
            }
        }
        Ok(progress)
    }
    pub fn startup_complete(&self) -> bool {
        self.cursor == self.startup.len()
    }
    pub fn eligible(&self) -> bool {
        self.startup_complete()
            && self.programs.iter().all(|program| {
                let Some(id) = program.current else {
                    return true;
                };
                self.instance(id).retired()
                    && (program.declaration.control().ending == d::Ending::Transient
                        || !program.run_requested)
            })
            && self
                .creates
                .iter()
                .all(|record| self.instance(record.instance).retired())
    }
    pub fn begin_close(&mut self) {
        self.closing = true;
        for program in &mut self.programs {
            program.run_requested = false;
        }
        self.close_round();
    }
    fn close_round(&mut self) {
        if !self.closing {
            return;
        }
        let canonical: Vec<_> = self
            .programs
            .iter()
            .filter_map(|program| program.current)
            .collect();
        for instance in &mut self.instances {
            if !canonical.contains(&instance.id) && !instance.retired() {
                let _ = instance.request(Intent::Stop);
            }
        }
        if self
            .instances
            .iter()
            .any(|instance| !canonical.contains(&instance.id) && !instance.retired())
        {
            return;
        }
        let stoppable: Vec<_> = self
            .programs
            .iter()
            .filter_map(|provider| {
                let id = provider.current?;
                let name = provider.declaration.name();
                let depended_on = self.programs.iter().any(|consumer| {
                    consumer
                        .current
                        .is_some_and(|other| other != id && !self.instance(other).retired())
                        && (consumer.declaration.control().requires.contains(&name)
                            || consumer
                                .declaration
                                .control()
                                .retirement
                                .iter()
                                .any(|retirement| retirement.provider == name))
                });
                (!depended_on).then_some(id)
            })
            .collect();
        for instance in &mut self.instances {
            if stoppable.contains(&instance.id) && !instance.retired() {
                let _ = instance.request(Intent::Stop);
            }
        }
    }
    pub fn closed(&self) -> bool {
        self.instances.iter().all(Instance::retired)
    }
    pub fn cached_entries(&self) -> usize {
        self.loader.cached_entries()
    }
    pub fn finish_cache(&mut self) -> bool {
        self.loader.clear_images().unwrap_or_else(|_| {
            crate::system::app::services::fatal("system: cache retirement failed")
        })
    }
    fn observed(&self, index: usize) -> api::Observation {
        self.programs[index]
            .current
            .map(|id| self.instance(id).observation())
            .unwrap_or(api::Observation {
                state: api::State::NeverStarted,
                reason: 0,
                completed: false,
                task: system_protocol::TaskId::NONE,
                team: system_protocol::TeamId::NONE,
            })
    }
    fn authorize_named(
        &self,
        services: &Services,
        caller: TaskId,
        index: usize,
        recover: bool,
    ) -> Result<(), api::Fail> {
        if caller == services.caller {
            return Ok(());
        }
        let source = self.caller_unit(caller).ok_or(api::Fail::Denied)?;
        let target = &self.programs[index];
        if source.name() == target.declaration.name() && !recover {
            return Ok(());
        }
        let group = source.control().supervision.ok_or(api::Fail::Denied)?;
        if !target.declaration.control().groups.contains(&group)
            || !target.run_requested
            || target.declaration.control().guarded
        {
            return Err(api::Fail::Denied);
        }
        Ok(())
    }
    pub fn command(
        &mut self,
        services: &Services,
        caller: TaskId,
        grant: api::Grant,
        request: api::operations::DecodedRequest,
    ) -> api::operations::Response {
        use api::operations::{DecodedRequest as Q, Response as R};
        macro_rules! answer {
            ($variant:ident, $value:expr) => {
                R::$variant(match $value {
                    Ok(value) => Outcome::Success(value),
                    Err(error) => Outcome::Failure(error),
                })
            };
        }
        let named = |this: &Self, value: &api::Named| {
            this.slot(value.name.as_str()).ok_or(api::Fail::Unknown)
        };
        macro_rules! named {
            ($variant:ident, $value:expr, $intent:expr) => {{
                let value = $value;
                let result = (|| {
                    let at = named(self, &value)?;
                    self.authorize_named(services, caller, at, false)?;
                    let id = self.programs[at].current.ok_or(api::Fail::NotReady)?;
                    self.instances
                        .iter_mut()
                        .find(|item| item.id == id)
                        .unwrap()
                        .request($intent)?;
                    if $intent == Intent::Stop {
                        self.programs[at].run_requested = false;
                    }
                    Ok(self.observed(at))
                })();
                answer!($variant, result)
            }};
        }
        macro_rules! instance {
            ($variant:ident, $value:expr, $intent:expr) => {{
                let value = $value;
                let result = (|| {
                    let instance = self
                        .instances
                        .iter_mut()
                        .find(|item| item.observation().task == value.task)
                        .ok_or(api::Fail::Unknown)?;
                    if caller != services.caller && !instance.owns_caller(caller) {
                        return Err(api::Fail::Denied);
                    }
                    if let Some(intent) = $intent {
                        instance.request(intent)?;
                    }
                    Ok(instance.observation())
                })();
                answer!($variant, result)
            }};
        }
        // Endpoint role is live native authority, separate from body observations.
        let required = match &request {
            Q::Mint(_) => api::Grant::Mint,
            Q::Embark(_) | Q::EmbarkInstance(_) => api::Grant::Embark,
            Q::Debark(_) | Q::DebarkInstance(_) => api::Grant::Debark,
            Q::Ruin(_) | Q::RuinInstance(_) | Q::ReleaseInstance(_) => api::Grant::Ruin,
            Q::Recover(_) => api::Grant::Recover,
            Q::Observe(_) => api::Grant::Observe,
            Q::State(_) | Q::StateInstance(_) | Q::Task(_) | Q::Publisher(_) => api::Grant::State,
        };
        if required != grant {
            return deny_control(request);
        }
        match request {
            Q::State(value) => answer!(State, named(self, &value).map(|at| self.observed(at))),
            Q::Task(value) => answer!(Task, named(self, &value).map(|at| self.observed(at))),
            Q::Mint(value) => {
                let result = (|| {
                    let at = named(self, &value)?;
                    self.authorize_named(services, caller, at, false)?;
                    if !self.programs[at].run_requested {
                        self.mint(at, services, false)?;
                    }
                    Ok(self.observed(at))
                })();
                answer!(Mint, result)
            }
            Q::Embark(value) => {
                let result = (|| {
                    let at = named(self, &value)?;
                    self.authorize_named(services, caller, at, false)?;
                    let id = if !self.programs[at].run_requested {
                        self.mint(at, services, true)?
                    } else {
                        self.programs[at].current.ok_or(api::Fail::NotReady)?
                    };
                    self.instances
                        .iter_mut()
                        .find(|item| item.id == id)
                        .unwrap()
                        .request(Intent::Run)?;
                    Ok(self.observed(at))
                })();
                answer!(Embark, result)
            }
            Q::Debark(value) => named!(Debark, value, Intent::Freeze),
            Q::Ruin(value) => named!(Ruin, value, Intent::Stop),
            Q::EmbarkInstance(value) => instance!(EmbarkInstance, value, Some(Intent::Run)),
            Q::DebarkInstance(value) => instance!(DebarkInstance, value, Some(Intent::Freeze)),
            Q::RuinInstance(value) => instance!(RuinInstance, value, Some(Intent::Stop)),
            Q::ReleaseInstance(value) => instance!(ReleaseInstance, value, Some(Intent::Stop)),
            Q::StateInstance(value) => instance!(StateInstance, value, None::<Intent>),
            Q::Recover(value) => {
                let result = (|| {
                    let at = named(self, &value)?;
                    self.authorize_named(services, caller, at, true)?;
                    if self.programs[at]
                        .current
                        .is_none_or(|id| !self.instance(id).retired())
                    {
                        return Err(api::Fail::NotReady);
                    }
                    self.mint(at, services, true)?;
                    Ok(self.observed(at))
                })();
                answer!(Recover, result)
            }
            Q::Observe(value) => {
                let row = self.programs.get(value.index as usize);
                answer!(
                    Observe,
                    Ok::<_, api::Fail>(api::Row {
                        index: value.index,
                        present: row.is_some(),
                        end: value.index as usize + 1 >= self.programs.len(),
                        run_requested: row.is_some_and(|row| row.run_requested),
                        name: Optional(
                            row.and_then(|row| system_protocol::Name::new(row.declaration.name()))
                        ),
                        state: row.map_or(api::State::Dead, |_| self
                            .observed(value.index as usize)
                            .state)
                    })
                )
            }
            Q::Publisher(value) => {
                let result = (|| {
                    let unit = self.caller_unit(caller).ok_or(api::Fail::Denied)?;
                    let instance = self
                        .instances
                        .iter()
                        .find(|instance| {
                            instance.ready()
                                && instance.contains_caller(caller)
                                && instance
                                    .program
                                    .as_ref()
                                    .is_some_and(|program| program.name() == unit.name())
                        })
                        .ok_or(api::Fail::Denied)?;
                    let at = unit
                        .namespaces()
                        .iter()
                        .position(|ns| Mark::of(ns.key).get() == value.namespace.get())
                        .ok_or(api::Fail::Denied)?;
                    let ns = &unit.namespaces()[at];
                    let publication = ns
                        .publications
                        .iter()
                        .find(|publication| publication.mark.get() == value.resource.get())
                        .ok_or(api::Fail::Denied)?;
                    let resource = self
                        .layout
                        .resource(
                            services,
                            instance.anchor(at).ok_or(api::Fail::NotReady)?,
                            publication.name.as_str(),
                        )
                        .map_err(|_| api::Fail::NotReady)?;
                    let token = PendId::try_from(resource.token).map_err(|_| api::Fail::Bad)?;
                    let delivered = give(token, caller, publication.mark, publication.limits)
                        .map_err(|_| api::Fail::Denied)?;
                    let deadline = kernel_client::call::room::clock()
                        .saturating_add(crate::system::app::services::LIMIT_NS);
                    while !crate::system::app::services::release(token) {
                        crate::system::app::services::bound(
                            deadline,
                            "system: publication read retirement timeout",
                        );
                    }
                    Ok(delivered.into())
                })();
                answer!(Publisher, result)
            }
        }
    }
    pub fn build(
        &mut self,
        services: &Services,
        caller: TaskId,
        request: build::operations::DecodedRequest,
    ) -> build::operations::Response {
        use build::operations::{DecodedRequest as Q, Response as R};
        macro_rules! answer {
            ($variant:ident, $value:expr) => {
                R::$variant(match $value {
                    Ok(value) => Outcome::Success(value),
                    Err(error) => Outcome::Failure(error),
                })
            };
        }
        match request {
            Q::Build(request) => {
                let result = (|| {
                    if let Some(record) = self
                        .creates
                        .iter()
                        .find(|row| row.caller == caller && row.request.id == request.id)
                    {
                        if record.request != request {
                            return Err(build::Fail::Denied);
                        }
                        return self.construction(services, caller, request.id);
                    }
                    if self.creates.len() == CAPACITY {
                        return Err(build::Fail::Full);
                    }
                    let origin = self.caller_unit(caller).ok_or(build::Fail::Denied)?;
                    let (name, args, stack, size) = match &request.input {
                        build::Input::Unit(input) => (
                            Some(input.name.as_str()),
                            &input.arguments.values,
                            input.stack,
                            self.catalog
                                .find(input.name.as_str())
                                .and_then(|unit| self.images.find(unit.loader().image))
                                .map_or(0, |file| file.bytes.len()),
                        ),
                        build::Input::Image(input) => (
                            None,
                            &input.arguments.values,
                            input.stack,
                            input.len as usize,
                        ),
                    };
                    let policy = origin
                        .control()
                        .construction
                        .iter()
                        .find(|policy| {
                            policy.unit == name
                                && args.len() >= usize::from(policy.arguments.min)
                                && args.len() <= usize::from(policy.arguments.max)
                                && stack >= policy.stack.min
                                && stack <= policy.stack.max
                                && (name.is_some() || size as u64 <= policy.max_image)
                        })
                        .ok_or(build::Fail::Denied)?;
                    let owner = match policy.owner {
                        d::Owner::Caller => caller,
                        d::Owner::Selected => TaskId::new(
                            usize::try_from(request.owner.get())
                                .map_err(|_| build::Fail::Denied)?,
                        ),
                        d::Owner::Task(task) => {
                            TaskId::new(usize::try_from(task).map_err(|_| build::Fail::Denied)?)
                        }
                        d::Owner::Unit(name) => self
                            .slot(name)
                            .and_then(|at| self.programs[at].current)
                            .and_then(|id| self.instance(id).task())
                            .ok_or(build::Fail::Denied)?,
                    };
                    if request.owner.get() != owner.get() as u64
                        || lookup(owner).is_none_or(|info| info.state == UnitState::Exited)
                    {
                        return Err(build::Fail::Denied);
                    }
                    let caller_subject = self
                        .roster
                        .subject(services, caller)
                        .map_err(|_| build::Fail::Denied)?;
                    let identity = match &policy.identity {
                        d::IdentitySelection::Caller if request.subject == caller_subject => {
                            id::Install::Inherit(id::Task {
                                task: caller.into(),
                            })
                        }
                        d::IdentitySelection::Caller => return Err(build::Fail::Denied),
                        d::IdentitySelection::Selected => id::Install::Restrict(id::Restriction {
                            parent: caller.into(),
                            subject: request.subject.clone(),
                        }),
                        d::IdentitySelection::Exact(subject) => {
                            let expected = crate::system::declaration::subject_value(subject)
                                .map_err(|_| build::Fail::Denied)?;
                            if expected != request.subject {
                                return Err(build::Fail::Denied);
                            }
                            id::Install::Authorized(expected)
                        }
                    };
                    let values: Vec<_> = args
                        .iter()
                        .map(|value| usize::try_from(*value).map_err(|_| build::Fail::Bad))
                        .collect::<Result<_, _>>()?;
                    let program = name.and_then(|name| self.catalog.find(name));
                    let snapshot;
                    let bytes = match &request.input {
                        build::Input::Unit(_) => {
                            self.images
                                .find(program.as_ref().ok_or(build::Fail::Unknown)?.loader().image)
                                .ok_or(build::Fail::BadImage)?
                                .bytes
                        }
                        build::Input::Image(input) => {
                            snapshot = construction::snapshot(services, caller, input)?;
                            &snapshot
                        }
                    };
                    let instance = self
                        .spawn(owner, program, bytes, &values, stack as usize, identity)
                        .map_err(build_fail)?;
                    self.creates.push(Creation {
                        caller,
                        request: request.clone(),
                        instance,
                        resolve: None,
                        owner_resolve: None,
                        claimed: false,
                        expires: kernel_client::call::room::clock().saturating_add(5_000_000_000),
                    });
                    self.construction(services, caller, request.id)
                })();
                answer!(Build, result)
            }
            Q::Inspect(id) => answer!(Inspect, self.construction(services, caller, id)),
            Q::Cancel(id) => {
                let result = (|| {
                    let record = self
                        .creates
                        .iter()
                        .find(|row| row.caller == caller && row.request.id == id)
                        .ok_or(build::Fail::Unknown)?;
                    self.instances
                        .iter_mut()
                        .find(|instance| instance.id == record.instance)
                        .unwrap()
                        .request(Intent::Stop)
                        .map_err(build_fail)?;
                    self.construction(services, caller, id)
                })();
                answer!(Cancel, result)
            }
            Q::Claim(request) => {
                let value = (|| {
                    let at = self
                        .creates
                        .iter()
                        .position(|record| {
                            self.instance(record.instance).observation().task == request.task
                                && self.instance(record.instance).owner == caller
                        })
                        .ok_or(build::Fail::Denied)?;
                    let instance = self.instance(self.creates[at].instance);
                    if !instance.constructed() {
                        return Err(build::Fail::NotReady);
                    }
                    let observed = instance.observation();
                    if self.creates[at].owner_resolve.is_none() {
                        self.creates[at].owner_resolve = Some(
                            give(
                                services.identity_entry(id::Grant::Resolve),
                                caller,
                                Mark::of("system/identity/resolve"),
                                Limits {
                                    operations: Operations::Clone(
                                        CloneRights::PUSH | CloneRights::PEEK,
                                    ),
                                    modifiable: Modifiable::Yes,
                                    distributable: Distributable::No,
                                    scope: PendScope::Object,
                                },
                            )
                            .map_err(|_| build::Fail::Full)?
                            .into(),
                        );
                    }
                    self.creates[at].claimed = true;
                    Ok(build::Built {
                        task: observed.task,
                        team: observed.team,
                        resolve: self.creates[at].owner_resolve.unwrap(),
                    })
                })();
                answer!(Claim, value)
            }
        }
    }
    fn construction(
        &mut self,
        services: &Services,
        caller: TaskId,
        id: build::CreateId,
    ) -> Result<build::Construction, build::Fail> {
        let at = self
            .creates
            .iter()
            .position(|row| row.caller == caller && row.request.id == id)
            .ok_or(build::Fail::Unknown)?;
        let instance = self.instance(self.creates[at].instance);
        let state = if instance.retired() {
            if instance.failure().is_some() {
                build::State::Failed
            } else {
                build::State::Closed
            }
        } else if instance.constructed() {
            build::State::Completed
        } else {
            build::State::Pending
        };
        let observed = instance.observation();
        let failure = instance.failure().map(build_fail);
        if state == build::State::Completed && self.creates[at].resolve.is_none() {
            self.creates[at].resolve = Some(
                give(
                    services.identity_entry(id::Grant::Resolve),
                    caller,
                    Mark::of("system/identity/resolve"),
                    Limits {
                        operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
                        modifiable: Modifiable::Yes,
                        distributable: Distributable::No,
                        scope: PendScope::Object,
                    },
                )
                .map_err(|_| build::Fail::Full)?
                .into(),
            );
        }
        Ok(build::Construction {
            state,
            built: Optional((state == build::State::Completed).then(|| build::Built {
                team: observed.team,
                task: observed.task,
                resolve: self.creates[at].resolve.unwrap(),
            })),
            failure: Optional(failure.map(weave::Coded)),
        })
    }
}
fn build_fail(error: api::Fail) -> build::Fail {
    match error {
        api::Fail::BadImage => build::Fail::BadImage,
        api::Fail::Full => build::Fail::Full,
        api::Fail::Unknown => build::Fail::Unknown,
        api::Fail::NotReady => build::Fail::NotReady,
        api::Fail::Bad => build::Fail::Bad,
        _ => build::Fail::Denied,
    }
}
fn deny_control(request: api::operations::DecodedRequest) -> api::operations::Response {
    use api::operations::{DecodedRequest as Q, Response as R};
    macro_rules! deny { ($($op:ident),*) => { match request { $(Q::$op(_) => R::$op(Outcome::Failure(api::Fail::Denied)),)* } }; }
    deny!(
        Mint,
        Embark,
        Debark,
        State,
        Ruin,
        EmbarkInstance,
        DebarkInstance,
        RuinInstance,
        StateInstance,
        Task,
        ReleaseInstance,
        Recover,
        Publisher,
        Observe
    )
}

impl Control {
    pub fn supply_policy(&self, caller: TaskId) -> Vec<system_protocol::supply::Binding> {
        self.caller_unit(caller)
            .map_or_else(Vec::new, |unit| unit.supply().to_vec())
    }
    pub fn export(
        &self,
        services: &Services,
        caller: TaskId,
        request: id::Export,
    ) -> Outcome<(), id::Fail> {
        let value = (|| {
            let unit = self.caller_unit(caller).ok_or(id::Fail::Denied)?;
            if !unit.identity().aliases.contains(&request.alias.as_str()) {
                return Err(id::Fail::Denied);
            }
            let id::IdentityObject::Principal(principal) = request.object else {
                return Err(id::Fail::Denied);
            };
            let subject = self.roster.subject(services, caller)?;
            if *subject.principal() != principal {
                return Err(id::Fail::Denied);
            }
            let instance = self
                .instances
                .iter()
                .find(|instance| instance.ready() && instance.contains_caller(caller))
                .ok_or(id::Fail::Denied)?;
            let mut proven = false;
            for index in 0..unit.namespaces().len() {
                let Some(anchor) = instance.anchor(index) else {
                    continue;
                };
                if let Ok(resource) =
                    self.layout
                        .resource(services, anchor, request.binding.as_str())
                {
                    proven |= resource.binding.entry.operator == request.entry.operator
                        && resource.binding.revision == request.entry.revision
                        && match resource.binding.entry.target {
                            system_protocol::operator::EntryTarget::Node(id) => {
                                !request.entry.root && id == request.entry.id
                            }
                            system_protocol::operator::EntryTarget::Root => false,
                        };
                    let token = PendId::try_from(resource.token).map_err(|_| id::Fail::Denied)?;
                    let deadline = kernel_client::call::room::clock()
                        .saturating_add(crate::system::app::services::LIMIT_NS);
                    while !crate::system::app::services::release(token) {
                        crate::system::app::services::bound(
                            deadline,
                            "system: export read retirement timeout",
                        );
                    }
                }
            }
            if !proven {
                return Err(id::Fail::Denied);
            }
            result(services.identity::<id::operations::Associate>(
                id::Grant::Install,
                id::Associate {
                    object: request.object,
                    name: request.binding,
                    entry: request.entry,
                },
            ))
        })();
        match value {
            Ok(()) => Outcome::Success(()),
            Err(error) => Outcome::Failure(error),
        }
    }
}

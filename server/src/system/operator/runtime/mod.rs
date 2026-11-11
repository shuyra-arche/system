//! Native Operator Task. The serial owner coordinates namespace commits,
//! grant publication and retryable retirement without replaying failed replies.
mod face;
use crate::system::{
    app::waiting::{Interest, Waiting},
    operator::{
        authority::{CleanupStep, GrantKey, GrantRef, GrantSubmission, Retirement, RootGrant},
        tree::{
            Context, Operator,
            gate::{Court, Unavailable, verdict},
            journal::{Intent, Key},
        },
        watch::Watchers,
    },
};
use alloc::vec::Vec;
use core::fmt;
use face::{Face, Ingress, Role};
use kernel_api::{
    abi::call::{pend::PendFail, sign::SignReply, unit::UnitFail},
    address::VirtAddr,
    model::{
        identity::{PendId, TaskId},
        pend::{Distributable, PendInfo, PendStatus},
        sign::SignSource,
        unit::{UnitInfo, UnitQuery, UnitTarget},
        wait::Wait,
    },
};
use kernel_client::call::{pend, room, sign, unit};
use system_client::identity::TaskQuery;
use system_protocol::{
    operator::{self as api, Fail, GrantState, Ops},
    value::Outcome,
};
use weave::{server::ServeError, transport::ReceiveError};

pub struct Config {
    pub license: PendId,
    pub controller: TaskId,
    pub shutdown: PendId,
}
#[derive(Clone, Copy, Debug)]
pub struct Publication {
    pub discovery: system_protocol::PendId,
    pub authority: system_protocol::PendId,
    pub setup: system_protocol::PendId,
}
#[derive(Debug)]
pub enum Error {
    InvalidConfig,
    Inspect(erra::Error<PendFail>),
    Unit(erra::Error<UnitFail>),
    Face(face::Error),
    Waiting(crate::system::app::waiting::Error),
    Book(Fail),
    Ingress(crate::protocol::Error),
    Capacity,
    Inactive,
}
enum Phase {
    Starting,
    Running,
    Closing,
    Closed,
}
#[derive(Default, Debug)]
pub struct Report {
    pub handled: usize,
    pub rejected: usize,
    pub reply_failures: usize,
    pub cleanup_blocked: bool,
    pub stopped: bool,
}
#[must_use = "finish the retained native service, retrying failed cleanup"]
pub struct Service {
    config: Config,
    task: TaskId,
    phase: Phase,
    book: Option<Operator>,
    watchers: Option<Watchers>,
    faces: Vec<Option<Face>>,
    waiting: Option<Waiting>,
    retiring: Vec<Retirement>,
    retire_cursor: usize,
    cursor: usize,
    processing: Option<usize>,
    publication: Option<Publication>,
}
impl fmt::Debug for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Operator")
            .field("task", &self.task)
            .field("retiring", &self.retiring.len())
            .field("closed", &self.is_finished())
            .finish()
    }
}
#[derive(Debug)]
#[must_use = "finish the retained service after construction or progress fails"]
pub struct Failure {
    pub error: Error,
    pub service: Service,
}
impl Service {
    #[allow(
        clippy::result_large_err,
        reason = "return partial ownership without allocating on failure"
    )]
    pub fn create(config: Config) -> Result<Self, Failure> {
        let mut service = Self {
            config,
            task: TaskId::new(0),
            phase: Phase::Starting,
            book: None,
            watchers: None,
            faces: Vec::new(),
            waiting: None,
            retiring: Vec::new(),
            retire_cursor: 0,
            cursor: 0,
            processing: None,
            publication: None,
        };
        match service.initialize() {
            Ok(()) => {
                service.phase = Phase::Running;
                Ok(service)
            }
            Err(error) => Err(Failure { error, service }),
        }
    }
    fn initialize(&mut self) -> Result<(), Error> {
        let mut bytes = [0; UnitInfo::WIDTH];
        if !unit::inspect(UnitQuery::Caller, VirtAddr::new(bytes.as_mut_ptr().addr()))
            .map_err(Error::Unit)?
        {
            return Err(Error::InvalidConfig);
        }
        let UnitTarget::Task(task) = UnitInfo::from_bytes(&bytes)
            .ok_or(Error::InvalidConfig)?
            .target
        else {
            return Err(Error::InvalidConfig);
        };
        self.task = task;
        if task == self.config.controller || self.config.controller.get() == 0 {
            return Err(Error::InvalidConfig);
        }
        let shutdown = inspect(self.config.shutdown)?;
        use kernel_api::model::{pend::Operations, sign::SignRights};
        if shutdown.owner != self.config.controller
            || !matches!(shutdown.limits.operations, Operations::Sign(rights) if rights.contains(SignRights::LISTEN | SignRights::CONSUME))
        {
            return Err(Error::InvalidConfig);
        }
        self.book =
            Some(Operator::new(self.config.controller.into(), task.into()).map_err(Error::Book)?);
        self.watchers = Some(Watchers::new().map_err(Error::Book)?);
        self.faces
            .try_reserve_exact(259)
            .map_err(|_| Error::Capacity)?;
        self.faces.resize_with(259, || None);
        self.retiring
            .try_reserve_exact(256)
            .map_err(|_| Error::Capacity)?;
        self.waiting = Some(Waiting::new(self.config.license).map_err(Error::Waiting)?);
        for (index, role) in [Role::Discovery, Role::Root, Role::Setup]
            .into_iter()
            .enumerate()
        {
            self.faces[index] = Some(Face::new(role));
            let face = self.faces[index].as_mut().unwrap();
            face.initialize(self.config.license).map_err(Error::Face)?;
            face.deliver(self.config.controller, Distributable::Yes)
                .map_err(Error::Face)?;
        }
        self.reconcile()?;
        self.publication = Some(Publication {
            discovery: self.faces[0].as_ref().unwrap().remote.unwrap(),
            authority: self.faces[1].as_ref().unwrap().remote.unwrap(),
            setup: self.faces[2].as_ref().unwrap().remote.unwrap(),
        });
        Ok(())
    }
    pub fn task(&self) -> TaskId {
        self.task
    }
    pub fn publication(&self) -> Option<Publication> {
        if matches!(self.phase, Phase::Running) {
            self.publication
        } else {
            None
        }
    }
    pub fn is_finished(&self) -> bool {
        matches!(self.phase, Phase::Closed)
    }
    fn reconcile(&mut self) -> Result<(), Error> {
        let mut roots = Vec::new();
        roots
            .try_reserve_exact(self.faces.len())
            .map_err(|_| Error::Capacity)?;
        roots.extend(
            self.faces
                .iter()
                .flatten()
                .filter(|face| !face.closing && face.ingress.is_some())
                .filter_map(|face| face.root),
        );
        let extra = [
            Interest::new(SignSource::Sign {
                token: self.config.shutdown,
            }),
            Interest::new(SignSource::Inspect {
                task: self.task,
                token: self.config.shutdown,
            }),
        ];
        self.waiting
            .as_mut()
            .ok_or(Error::Inactive)?
            .apply(&roots, &[], &extra)
            .map_err(Error::Waiting)
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        self.phase = Phase::Closing;
        if let Some(waiting) = self.waiting.as_mut() {
            waiting.finish().map_err(Error::Waiting)?;
            self.waiting = None;
        }
        for face in self.faces.iter_mut().flatten() {
            face.close_ingress().map_err(Error::Face)?;
        }
        if let Some(watchers) = self.watchers.as_mut() {
            watchers.finish().map_err(Error::Book)?;
        }
        if let Some(book) = self.book.as_mut() {
            book.finish().map_err(Error::Book)?;
        }
        for face in self.faces.iter_mut().flatten() {
            face.finish().map_err(Error::Face)?;
        }
        self.publication = None;
        self.phase = Phase::Closed;
        Ok(())
    }
    pub fn step(&mut self, wait: Wait) -> Result<Report, Error> {
        if !matches!(self.phase, Phase::Running) {
            return Err(Error::Inactive);
        }
        inspect(self.config.shutdown)?;
        match sign::listen(self.config.shutdown, Wait::Poll).map_err(|_| Error::InvalidConfig)? {
            SignReply::Triggered { sequence } => {
                sign::consume(self.config.shutdown, sequence).map_err(|_| Error::InvalidConfig)?;
                self.phase = Phase::Closing;
                return Ok(Report {
                    stopped: true,
                    ..Report::default()
                });
            }
            SignReply::Pending => (),
            _ => return Err(Error::InvalidConfig),
        }
        let mut retained = !self.book.as_mut().unwrap().sweep();
        retained |= self.watchers.as_mut().unwrap().sweep();
        let mut report = Report::default();
        for offset in 0..self.faces.len() {
            let index = (self.cursor + offset) % self.faces.len();
            let Some(mut face) = self.faces[index].take() else {
                continue;
            };
            if face.closing {
                self.faces[index] = Some(face);
                continue;
            }
            if let Role::Grant { key, .. } = face.role
                && !self.book.as_ref().unwrap().authority.live(key)
            {
                self.faces[index] = Some(face);
                continue;
            }
            self.processing = Some(index);
            let root = face.root;
            let role = face.role;
            let result = match face.ingress.as_mut() {
                Some(Ingress::Discovery(endpoint))
                    if !self.watchers.as_ref().unwrap().blocked() =>
                {
                    let result = endpoint
                        .serve_once(Wait::Poll, |caller, request| self.discover(caller, request));
                    retained |= endpoint.needs_retry();
                    result
                }
                Some(Ingress::Authority(endpoint)) if !self.book.as_ref().unwrap().blocked() => {
                    let result = endpoint.serve_once(Wait::Poll, |caller, request| {
                        self.authorize(caller, root.unwrap(), role, request)
                    });
                    retained |= endpoint.needs_retry();
                    result
                }
                Some(Ingress::Setup(endpoint)) => {
                    let result = endpoint
                        .serve_once(Wait::Poll, |caller, request| self.setup(caller, request));
                    retained |= endpoint.needs_retry();
                    result
                }
                _ => {
                    self.processing = None;
                    self.faces[index] = Some(face);
                    continue;
                }
            };
            self.processing = None;
            self.faces[index] = Some(face);
            match result {
                Ok(()) => report.handled += 1,
                Err(crate::protocol::Error::NotReady) => (),
                Err(crate::protocol::Error::Serve(ServeError::Receive(ReceiveError::Decode)))
                | Err(crate::protocol::Error::Serve(ServeError::Route(_))) => report.rejected += 1,
                Err(crate::protocol::Error::Serve(ServeError::Send(_))) => {
                    report.handled += 1;
                    report.reply_failures += 1;
                }
                Err(error) => return Err(Error::Ingress(error)),
            }
        }
        self.cursor = (self.cursor + 1) % self.faces.len();
        retained |= self.progress_grants()?;
        report.cleanup_blocked = self.progress_retirement()?;
        retained |= report.cleanup_blocked;
        for event in self.book.as_mut().unwrap().changes() {
            self.watchers.as_mut().unwrap().publish(&event);
        }
        if report.handled + report.rejected == 0 {
            if retained {
                room::yield_();
            } else if wait != Wait::Poll {
                self.waiting
                    .as_ref()
                    .ok_or(Error::Inactive)?
                    .await_(wait)
                    .map_err(Error::Waiting)?;
            }
        }
        Ok(report)
    }
    fn setup(
        &mut self,
        caller: TaskId,
        request: api::setup::operations::DecodedRequest,
    ) -> api::setup::operations::Response {
        use api::setup::operations::{DecodedRequest as Request, Response};
        match request {
            Request::Root(()) => Response::Root(outcome(if caller == self.config.controller {
                let entrance = self.faces[1].as_ref().unwrap().root.unwrap();
                self.book
                    .as_mut()
                    .unwrap()
                    .authority
                    .bootstrap_root(RootGrant {
                        system: caller.into(),
                        operator: self.task.into(),
                        entrance,
                    })
                    .map_err(authority_error)
            } else {
                Err(Fail::Denied)
            })),
            Request::Wired(wired) => {
                Response::Wired(outcome(if caller == self.config.controller {
                    TaskQuery::from_wired(wired, self.config.license)
                        .map_err(|_| Fail::Denied)
                        .and_then(|query| self.book.as_mut().unwrap().wire(query))
                } else {
                    Err(Fail::Denied)
                }))
            }
        }
    }
    fn judgment(&self, caller: TaskId, permit: api::Permit) -> Result<(), Fail> {
        let book = self.book.as_ref().unwrap();
        let court = Court {
            query: book.identity(),
            openers: |id| book.opener(id).map(Some).map_err(|_| Unavailable),
            wait: Wait::At(room::clock().saturating_add(1_000_000_000)),
        };
        match verdict(&court, caller.into(), permit).failure() {
            None => Ok(()),
            Some(error) => Err(error),
        }
    }
    fn discover(
        &mut self,
        caller: TaskId,
        request: api::operations::DecodedRequest,
    ) -> api::operations::Response {
        use api::operations::{DecodedRequest as Request, Response};
        let book = self.book.as_ref().unwrap();
        match request {
            Request::Seek(request) => {
                Response::Seek(outcome(book.seek(&book.root(), &request.path)))
            }
            Request::Relative(request) => {
                Response::Relative(outcome(book.seek(&request.base, &request.path)))
            }
            Request::Resolve(request) => {
                Response::Resolve(outcome(book.resolve(request.mark, &request.name)))
            }
            Request::List(request) => Response::List(outcome(book.list(&request.entry))),
            Request::Name(request) => Response::Name(outcome(book.name(&request.entry))),
            Request::Find(request) => Response::Find(outcome(
                book.permit(&request.entry)
                    .and_then(|permit| self.judgment(caller, permit))
                    .and_then(|()| {
                        self.book
                            .as_mut()
                            .unwrap()
                            .read(&request.entry, caller.into())
                    }),
            )),
            Request::Watch(request) => {
                Response::Watch(outcome(self.subscribe(caller, request, false)))
            }
            Request::WatchFrom(request) => {
                Response::WatchFrom(outcome(self.subscribe(caller, request, true)))
            }
        }
    }
    fn subscribe(
        &mut self,
        caller: TaskId,
        request: api::Watch,
        relative: bool,
    ) -> Result<system_protocol::Path, Fail> {
        self.judgment(caller, api::Permit::Bound)?;
        let book = self.book.as_ref().unwrap();
        let base = if relative {
            book.path(&request.base)?
        } else {
            if request.base != book.root() {
                return Err(Fail::Bad);
            }
            system_protocol::Path::new("").unwrap()
        };
        let path = if base.as_str().is_empty() {
            request.path
        } else if request.path.as_str().is_empty() {
            base.clone()
        } else {
            system_protocol::Path::new(&alloc::format!(
                "{}/{}",
                base.as_str(),
                request.path.as_str()
            ))
            .ok_or(Fail::Full)?
        };
        let token = request.endpoint.try_into().map_err(|_| Fail::Bad)?;
        if self.protected(token) || self.book.as_ref().unwrap().contains(token) {
            return Err(Fail::Denied);
        }
        self.watchers
            .as_mut()
            .unwrap()
            .join(caller.into(), token, path)?;
        Ok(base)
    }
    fn protected(&self, token: PendId) -> bool {
        token == self.config.license
            || token == self.config.shutdown
            || self
                .faces
                .iter()
                .flatten()
                .any(|face| face.root == Some(token))
    }
    fn authorize(
        &mut self,
        caller: TaskId,
        via: PendId,
        _role: Role,
        request: api::authority::operations::DecodedRequest,
    ) -> api::authority::operations::Response {
        use api::authority::operations::{DecodedRequest as Request, Response};
        let context = self
            .book
            .as_ref()
            .unwrap()
            .authority
            .grant_at(via, caller.into())
            .map(|(authority, _)| Context {
                caller: caller.into(),
                via,
                authority,
            });
        macro_rules! route {
            ($kind:ident, $body:expr) => {
                Response::$kind(outcome(context.ok_or(Fail::Denied).and_then($body)))
            };
        }
        match request {
            Request::Part(request) => {
                route!(Part, |context: Context| self
                    .mutate(context, Intent::Part(request)))
            }
            Request::Bind(request) => route!(Bind, |context: Context| self
                .publish(context, request, false)),
            Request::Replace(request) => {
                route!(Replace, |context: Context| self
                    .publish(context, request, true))
            }
            Request::Unbind(request) => {
                route!(Unbind, |context: Context| self
                    .remove(context, request, false))
            }
            Request::Unpart(request) => {
                route!(Unpart, |context: Context| self
                    .remove(context, request, true))
            }
            Request::Grant(spec) => route!(Grant, |context: Context| self.grant(context, spec)),
            Request::Inspect(id) => route!(Inspect, |context: Context| {
                let book = self.book.as_ref().unwrap();
                book.permission(context, Ops::INSPECT)?;
                book.authority
                    .status(GrantRef {
                        via,
                        caller: context.caller,
                        id,
                    })
                    .map_err(authority_error)
            }),
            Request::Revoke(id) => route!(Revoke, |context: Context| {
                self.book
                    .as_ref()
                    .unwrap()
                    .permission(context, Ops::REVOKE)?;
                self.revoke(context, id)
            }),
            Request::Associate(request) => route!(Associate, |context: Context| self
                .book
                .as_mut()
                .unwrap()
                .associate(context, request)),
            Request::ResolveNamespace(request) => route!(ResolveNamespace, |context: Context| {
                if context.authority != GrantKey::Root {
                    return Err(Fail::Denied);
                }
                self.book
                    .as_ref()
                    .unwrap()
                    .resolve(request.mark, &request.name)
            }),
            Request::Find(request) => route!(Find, |context: Context| {
                let book = self.book.as_ref().unwrap();
                book.authorized(context, &request.anchor, Ops::FIND, Some(&request.name))?;
                let entry = book
                    .child(&request.anchor, &request.name)?
                    .ok_or(Fail::Unknown)?;
                book.binding(&entry)
            }),
            Request::Read(request) => route!(Read, |context: Context| {
                let book = self.book.as_ref().unwrap();
                book.authorized(context, &request.entry, Ops::FIND, None)?;
                self.judgment(caller, book.permit(&request.entry)?)?;
                self.book
                    .as_mut()
                    .unwrap()
                    .read(&request.entry, context.caller)
            }),
            Request::InspectOperation(id) => route!(InspectOperation, |context: Context| {
                let book = self.book.as_ref().unwrap();
                book.permission(context, Ops::INSPECT)?;
                Ok(book.journal.inspect(Key {
                    via,
                    caller: context.caller,
                    id,
                }))
            }),
            Request::Cancel(id) => route!(Cancel, |context: Context| {
                let book = self.book.as_mut().unwrap();
                book.permission(context, Ops::CANCEL)?;
                book.journal.cancel(Key {
                    via,
                    caller: context.caller,
                    id,
                })
            }),
        }
    }
    fn mutate(&mut self, context: Context, intent: Intent) -> Result<api::Binding, Fail> {
        let book = self.book.as_mut().unwrap();
        let (key, replay) = book
            .journal
            .begin(context.via, context.caller, intent.clone())?;
        if let Some(status) = replay {
            return binding_result(status);
        }
        let result = match intent {
            Intent::Part(request) => book.part(context, &request),
            _ => Err(Fail::Bad),
        };
        book.journal.finish(key, result.clone())?;
        result
    }
    fn publish(
        &mut self,
        context: Context,
        request: api::Bind,
        replace: bool,
    ) -> Result<api::Binding, Fail> {
        let token = request.resource.try_into().map_err(|_| Fail::Bad)?;
        if self.protected(token) || self.watchers.as_ref().unwrap().contains(token) {
            return Err(Fail::Denied);
        }
        let book = self.book.as_mut().unwrap();
        let intent = Intent::publish(&request, replace);
        let begin = book.journal.begin(context.via, context.caller, intent);
        match begin {
            Ok((_, Some(status))) => {
                book.discard(context, &request)?;
                binding_result(status)
            }
            Ok((key, None)) => {
                let result = book.bind(context, &request, replace);
                book.journal.finish(key, result.clone())?;
                result
            }
            Err(error) => {
                book.discard(context, &request)?;
                Err(error)
            }
        }
    }
    fn remove(
        &mut self,
        context: Context,
        request: api::Mutation,
        pane: bool,
    ) -> Result<api::MutationStatus, Fail> {
        let book = self.book.as_mut().unwrap();
        let (key, replay) = book.journal.begin(
            context.via,
            context.caller,
            Intent::Remove {
                pane,
                change: request.clone(),
            },
        )?;
        if let Some(status) = replay {
            return Ok(status);
        }
        let result = book.remove(context, &request, pane);
        book.journal.finish(key, result)?;
        Ok(book.journal.inspect(key))
    }
    fn grant(&mut self, context: Context, spec: api::GrantSpec) -> Result<api::GrantStatus, Fail> {
        let reference = GrantRef {
            via: context.via,
            caller: context.caller,
            id: spec.id,
        };
        let free = self
            .faces
            .iter()
            .enumerate()
            .find(|(index, face)| face.is_none() && Some(*index) != self.processing)
            .map(|(index, _)| index)
            .ok_or(Fail::Full)?;
        let holder = spec.holder.try_into().map_err(|_| Fail::Bad)?;
        let (_, key) = self
            .book
            .as_mut()
            .unwrap()
            .grant(GrantSubmission {
                caller: context.caller,
                via: context.via,
                spec,
            })
            .map_err(authority_error)?;
        if !self
            .faces
            .iter()
            .flatten()
            .any(|face| matches!(face.role, Role::Grant { key: candidate, .. } if candidate == key))
        {
            self.faces[free] = Some(Face::new(Role::Grant {
                key,
                reference,
                holder,
            }));
        }
        self.book
            .as_ref()
            .unwrap()
            .authority
            .status(reference)
            .map_err(authority_error)
    }
    fn revoke(&mut self, context: Context, id: api::GrantId) -> Result<api::GrantStatus, Fail> {
        let reference = GrantRef {
            via: context.via,
            caller: context.caller,
            id,
        };
        let book = self.book.as_mut().unwrap();
        book.authority.revoke(reference).map_err(authority_error)?;
        for retirement in book
            .authority
            .revoke_plan(reference)
            .map_err(authority_error)?
        {
            if !self.retiring.iter().any(|item| item.key == retirement.key) {
                self.retiring.push(retirement);
            }
        }
        book.authority.status(reference).map_err(authority_error)
    }
    fn progress_grants(&mut self) -> Result<bool, Error> {
        let mut pending = false;
        for index in 3..self.faces.len() {
            let Some(face) = &self.faces[index] else {
                continue;
            };
            let Role::Grant {
                key,
                reference,
                holder,
            } = face.role
            else {
                continue;
            };
            let state = self
                .book
                .as_ref()
                .unwrap()
                .authority
                .inspect(reference)
                .map_err(|error| Error::Book(authority_error(error)))?;
            if state == GrantState::Revoking {
                let context = Context {
                    caller: reference.caller,
                    via: reference.via,
                    authority: key,
                };
                self.revoke(context, reference.id).map_err(Error::Book)?;
                continue;
            }
            if state != GrantState::Pending {
                continue;
            }
            pending = true;
            let result = self.faces[index]
                .as_mut()
                .unwrap()
                .initialize(self.config.license);
            match result {
                Ok(()) => (),
                Err(error) if transient(&error) => continue,
                Err(_) => {
                    self.book
                        .as_mut()
                        .unwrap()
                        .authority
                        .revoke(reference)
                        .map_err(|e| Error::Book(authority_error(e)))?;
                    continue;
                }
            }
            let root = self.faces[index].as_ref().unwrap().root.unwrap();
            self.book
                .as_mut()
                .unwrap()
                .authority
                .set_entrance(reference, root)
                .map_err(|e| Error::Book(authority_error(e)))?;
            self.reconcile()?;
            self.book
                .as_mut()
                .unwrap()
                .authority
                .attached(key)
                .map_err(|e| Error::Book(authority_error(e)))?;
            self.faces[index].as_mut().unwrap().attached = true;
            match self.faces[index]
                .as_mut()
                .unwrap()
                .deliver(holder, Distributable::No)
            {
                Ok(()) => self
                    .book
                    .as_mut()
                    .unwrap()
                    .authority
                    .delivered(
                        reference,
                        self.faces[index].as_ref().unwrap().remote.unwrap(),
                    )
                    .map_err(|e| Error::Book(authority_error(e)))?,
                Err(error) if transient(&error) => (),
                Err(_) => {
                    self.book
                        .as_mut()
                        .unwrap()
                        .authority
                        .revoke(reference)
                        .map_err(|e| Error::Book(authority_error(e)))?;
                }
            }
        }
        Ok(pending)
    }
    fn progress_retirement(&mut self) -> Result<bool, Error> {
        if self.retiring.is_empty() {
            return Ok(false);
        }
        let job = self.retire_cursor % self.retiring.len();
        self.retire_cursor = (job + 1) % self.retiring.len();
        let item = &self.retiring[job];
        let key = item.key;
        let reference = item.reference;
        let index = self.faces.iter().position(|face| face.as_ref().is_some_and(|face| matches!(face.role, Role::Grant { key: candidate, .. } if candidate == key))).ok_or(Error::InvalidConfig)?;
        self.faces[index].as_mut().unwrap().closing = true;
        if let Some(root) = self.faces[index].as_ref().unwrap().root
            && self
                .waiting
                .as_mut()
                .ok_or(Error::Inactive)?
                .detach(root)
                .is_err()
        {
            return Ok(true);
        }
        self.reconcile()?;
        if self.faces[index].as_mut().unwrap().close_ingress().is_err() {
            return Ok(true);
        }
        if self.book.as_mut().unwrap().withdraw(key).is_err() {
            return Ok(true);
        }
        self.book
            .as_mut()
            .unwrap()
            .authority
            .cleanup_step(key, CleanupStep::Detach)
            .map_err(|e| Error::Book(authority_error(e)))?;
        if self.faces[index].as_mut().unwrap().finish().is_err() {
            return Ok(true);
        }
        let book = self.book.as_mut().unwrap();
        book.authority
            .cleanup_step(key, CleanupStep::Return)
            .map_err(|e| Error::Book(authority_error(e)))?;
        match book.authority.closed(reference) {
            Ok(()) => (),
            Err(crate::system::operator::authority::AuthorityError::Denied) => return Ok(true),
            Err(error) => return Err(Error::Book(authority_error(error))),
        }
        if let Some(entrance) = self.retiring[job].entrance {
            book.journal.forget(entrance);
        }
        self.faces[index] = None;
        self.retiring.remove(job);
        self.retire_cursor = job % self.retiring.len().max(1);
        Ok(!self.retiring.is_empty())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
#[allow(clippy::result_large_err, reason = "return partial native ownership")]
pub fn serve(config: Config) -> Result<(), Failure> {
    let mut service = Service::create(config)?;
    loop {
        match service.step(Wait::Infinite) {
            Ok(report) if report.stopped => {
                return service.finish().map_err(|error| Failure { error, service });
            }
            Ok(_) => (),
            Err(error) => return Err(Failure { error, service }),
        }
    }
}
fn inspect(token: PendId) -> Result<PendInfo, Error> {
    let mut words = [0; PendInfo::WORDS];
    pend::inspect(token, VirtAddr::new(words.as_mut_ptr().addr())).map_err(Error::Inspect)?;
    let info = PendInfo::from_words(words).ok_or(Error::InvalidConfig)?;
    if info.status != PendStatus::Active {
        return Err(Error::InvalidConfig);
    }
    Ok(info)
}
fn outcome<T>(value: Result<T, Fail>) -> Outcome<T, Fail> {
    match value {
        Ok(value) => Outcome::Success(value),
        Err(error) => Outcome::Failure(error),
    }
}
fn binding_result(status: api::MutationStatus) -> Result<api::Binding, Fail> {
    if let Some(error) = status.failure.0 {
        return Err(error.0);
    }
    status.binding.0.ok_or(Fail::Conflict)
}
fn authority_error(error: crate::system::operator::authority::AuthorityError) -> Fail {
    use crate::system::operator::authority::AuthorityError as Error;
    match error {
        Error::Denied => Fail::Denied,
        Error::Conflict | Error::Stale => Fail::Conflict,
        Error::Full => Fail::Full,
        Error::Unknown => Fail::Unknown,
    }
}
fn transient(error: &face::Error) -> bool {
    match error {
        face::Error::Queue(error) => error.source.is_busy(),
        face::Error::Grant(error) | face::Error::Return(error) => error.source.is_busy(),
        _ => false,
    }
}

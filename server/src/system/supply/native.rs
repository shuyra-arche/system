//! Task-local native resource leases. Each lease owns a distinct private Pend
//! root; successful Return, rather than Stop or a reply, proves withdrawal.
use alloc::{string::String, vec::Vec};
use kernel_api::{
    abi::call::pend::PendFail,
    address::VirtAddr,
    model::{
        identity::{Mark, PendId, TaskId},
        pend::{
            AccordRequest, AccordTarget, Distributable, Limits, NestScope, PendInfo, PendRange,
            PendScope, PendStatus,
        },
    },
};
use kernel_client::call::pend;
use system_protocol::supply::{self as api, Fail, LeaseId, State};
use system_protocol::value::Optional;

const SOURCES: usize = 64;
const CALLERS: usize = 64;

pub struct Resource<'a> {
    pub name: &'a str,
    /// Borrowed source; registration creates and owns its own narrowed root.
    pub source: PendId,
    pub maximum: Limits,
    pub caller: TaskId,
    /// None permits the authorized caller to supply itself only.
    pub target: Option<TaskId>,
    pub description: &'a [u8],
}
struct Definition {
    name: String,
    root: Option<PendId>,
    maximum: Limits,
    caller: TaskId,
    target: Option<TaskId>,
    description: Vec<u8>,
}
struct Lease {
    caller: TaskId,
    request: api::Acquire,
    definition: usize,
    root: Option<PendId>,
    delivery: Option<api::Delivery>,
    state: State,
    failure: Option<Fail>,
}
struct Watermark {
    caller: TaskId,
    highest: u64,
}

#[must_use = "finish all private lease and registration roots before dropping"]
pub struct Source {
    key: String,
    definitions: Vec<Definition>,
    leases: Vec<Lease>,
    watermarks: Vec<Watermark>,
    cancelled: Vec<(TaskId, LeaseId)>,
    cursor: usize,
    closing: bool,
}
impl Source {
    pub fn new(key: &str) -> Result<Self, Fail> {
        if !valid_key(key) {
            return Err(Fail::Bad);
        }
        let mut definitions = Vec::new();
        definitions
            .try_reserve_exact(SOURCES)
            .map_err(|_| Fail::Full)?;
        let mut leases = Vec::new();
        leases
            .try_reserve_exact(api::MAX_LEASES)
            .map_err(|_| Fail::Full)?;
        let mut watermarks = Vec::new();
        watermarks
            .try_reserve_exact(CALLERS)
            .map_err(|_| Fail::Full)?;
        let mut cancelled = Vec::new();
        cancelled
            .try_reserve_exact(api::MAX_LEASES)
            .map_err(|_| Fail::Full)?;
        Ok(Self {
            key: String::from(key),
            definitions,
            leases,
            watermarks,
            cancelled,
            cursor: 0,
            closing: false,
        })
    }
    pub fn register(&mut self, value: Resource<'_>) -> Result<(), Fail> {
        if self.closing
            || !valid_key(value.name)
            || value.caller.get() == 0
            || value.description.len() > api::MAX_DESCRIPTION
            || !value.maximum.valid()
            || value.maximum.operations.words()[1] == 0
        {
            return Err(Fail::Bad);
        }
        if self.definitions.iter().any(|item| item.name == value.name) {
            return Err(Fail::Conflict);
        }
        if self.definitions.len() == SOURCES {
            return Err(Fail::Full);
        }
        // Allocate owned policy data before creating the registration branch.
        let name = String::from(value.name);
        let description = Vec::from(value.description);
        let mut limits = value.maximum;
        // This root is never exposed. Distribution is needed to create private
        // lease branches; the caller-facing maximum is checked separately.
        limits.distributable = Distributable::Yes;
        let root = accord(value.source, AccordTarget::Caller, Mark::NONE, limits)?;
        self.definitions.push(Definition {
            name,
            root: Some(root),
            maximum: value.maximum,
            caller: value.caller,
            target: value.target,
            description,
        });
        Ok(())
    }
    /// `caller` comes from the native ingress, never from a wire requester ID.
    /// Admission has no effects; advance owns all partial derivation/delivery.
    pub fn acquire(&mut self, caller: TaskId, request: api::Acquire) -> Result<api::Reply, Fail> {
        if let Some(old) = self
            .leases
            .iter()
            .find(|lease| lease.caller == caller && lease.request.lease == request.lease)
        {
            return if old.request == request {
                reply(old)
            } else {
                Err(Fail::Conflict)
            };
        }
        if self.cancelled.contains(&(caller, request.lease)) {
            return Ok(api::Reply {
                state: State::Closed,
                delivery: Optional(None),
            });
        }
        if self.closing {
            return Err(Fail::SourceLost);
        }
        if !request.valid()
            || request.binding.source.name != self.key
            || !request.binding.valid()
            || !request.binding.rights.0.valid()
            || request.target.get() > isize::MAX as u64
        {
            return Err(Fail::Bad);
        }
        let definition = self
            .definitions
            .iter()
            .position(|item| item.name == request.binding.resource.name)
            .ok_or(Fail::Unknown)?;
        let resource = &self.definitions[definition];
        let expected_target = resource.target.unwrap_or(caller);
        if resource.caller != caller
            || expected_target.get() as u64 != request.target.get()
            || !resource.maximum.permits_narrowing(request.binding.rights.0)
        {
            return Err(Fail::Denied);
        }
        let watermark = self
            .watermarks
            .iter()
            .position(|mark| mark.caller == caller);
        if watermark.is_some_and(|at| request.lease.get() <= self.watermarks[at].highest) {
            return Ok(api::Reply {
                state: State::Closed,
                delivery: Optional(None),
            });
        }
        if watermark.is_none() && self.watermarks.len() == CALLERS {
            return Err(Fail::Full);
        }
        if self.leases.len() == api::MAX_LEASES {
            // Compact completed receipts only under pressure. High-water marks
            // reject stale replay without recreating a grant or reusing an ID.
            for lease in self
                .leases
                .iter()
                .filter(|lease| lease.state == State::Released)
            {
                if let Some(mark) = self
                    .watermarks
                    .iter_mut()
                    .find(|mark| mark.caller == lease.caller)
                {
                    mark.highest = mark.highest.max(lease.request.lease.get());
                }
            }
            self.leases.retain(|lease| lease.state != State::Released);
        }
        if self.leases.len() == api::MAX_LEASES {
            return Err(Fail::Full);
        }
        if self
            .watermarks
            .iter()
            .any(|mark| mark.caller == caller && request.lease.get() <= mark.highest)
        {
            return Ok(api::Reply {
                state: State::Closed,
                delivery: Optional(None),
            });
        }
        match watermark {
            Some(_) => (),
            None => self.watermarks.push(Watermark { caller, highest: 0 }),
        }
        let delivery = Some(api::Delivery {
            token: system_protocol::PendId::NONE,
            description: resource.description.clone(),
        });
        self.leases.push(Lease {
            caller,
            request,
            definition,
            root: None,
            delivery,
            state: State::Acquiring,
            failure: None,
        });
        reply(self.leases.last().unwrap())
    }
    pub fn inspect(&self, caller: TaskId, id: LeaseId) -> Result<api::Reply, Fail> {
        if let Some(lease) = self
            .leases
            .iter()
            .find(|lease| lease.caller == caller && lease.request.lease == id)
        {
            return reply(lease);
        }
        if self.cancelled.contains(&(caller, id))
            || self
                .watermarks
                .iter()
                .any(|mark| mark.caller == caller && id.get() <= mark.highest)
        {
            Ok(api::Reply {
                state: State::Closed,
                delivery: Optional(None),
            })
        } else {
            Err(Fail::Unknown)
        }
    }
    pub fn release(&mut self, caller: TaskId, id: LeaseId) -> Result<api::Reply, Fail> {
        let Some(lease) = self
            .leases
            .iter_mut()
            .find(|lease| lease.caller == caller && lease.request.lease == id)
        else {
            return self.inspect(caller, id);
        };
        if lease.state != State::Released {
            lease.state = State::Releasing;
        }
        reply(lease)
    }
    /// Cancel-before-acquire closes this exact origin/ID; it cannot later mint.
    pub fn cancel(&mut self, caller: TaskId, id: LeaseId) -> Result<api::Reply, Fail> {
        if self
            .leases
            .iter()
            .any(|lease| lease.caller == caller && lease.request.lease == id)
        {
            return self.release(caller, id);
        }
        if caller.get() == 0 {
            return Err(Fail::Denied);
        }
        // A caller may close only its own authorized source scope.
        if !self.definitions.iter().any(|item| item.caller == caller) {
            return Err(Fail::Denied);
        }
        let caller_room = self.watermarks.len() < CALLERS;
        match self
            .watermarks
            .iter_mut()
            .find(|mark| mark.caller == caller)
        {
            Some(_) => (),
            None if caller_room => self.watermarks.push(Watermark { caller, highest: 0 }),
            None => return Err(Fail::Full),
        }
        if !self.cancelled.contains(&(caller, id)) {
            if self.cancelled.len() == api::MAX_LEASES {
                for (origin, id) in &self.cancelled {
                    if let Some(mark) = self
                        .watermarks
                        .iter_mut()
                        .find(|mark| mark.caller == *origin)
                    {
                        mark.highest = mark.highest.max(id.get());
                    }
                }
                self.cancelled.clear();
            }
            self.cancelled.push((caller, id));
        }
        Ok(api::Reply {
            state: State::Closed,
            delivery: Optional(None),
        })
    }
    /// One fair lease transition per round. Busy retains the exact current root.
    pub fn advance(&mut self) -> Result<(), Fail> {
        if self.leases.is_empty() {
            return Ok(());
        }
        let Some(at) = (0..self.leases.len())
            .map(|offset| (self.cursor + offset) % self.leases.len())
            .find(|at| self.leases[*at].state != State::Released)
        else {
            return Ok(());
        };
        self.cursor = (at + 1) % self.leases.len();
        let lease = &mut self.leases[at];
        let definition = &self.definitions[lease.definition];
        match lease.state {
            State::Acquiring if lease.root.is_none() => {
                let mut limits = lease.request.binding.rights.0;
                limits.distributable = Distributable::Yes;
                match accord(
                    definition.root.ok_or(Fail::SourceLost)?,
                    AccordTarget::Caller,
                    Mark::NONE,
                    limits,
                ) {
                    Ok(root) => lease.root = Some(root),
                    Err(Fail::Pending) => (),
                    Err(failure) => {
                        lease.failure = Some(failure);
                        lease.state = State::Releasing;
                    }
                }
            }
            State::Acquiring => {
                let mut limits = lease.request.binding.rights.0;
                // The private root already resolved the requested relative
                // window. Applying it a second time would shift the range.
                limits.scope = match limits.scope {
                    PendScope::Pages(_) => PendScope::Pages(PendRange::Inherit),
                    PendScope::Nest(NestScope::Reservation { id, .. }) => {
                        PendScope::Nest(NestScope::Reservation {
                            id,
                            range: PendRange::Inherit,
                        })
                    }
                    scope => scope,
                };
                let target = TaskId::new(
                    usize::try_from(lease.request.target.get()).map_err(|_| Fail::Bad)?,
                );
                match accord(
                    lease.root.unwrap(),
                    AccordTarget::Task(target),
                    Mark::new(lease.request.binding.slot.get()),
                    limits,
                ) {
                    Ok(token) => {
                        lease.delivery.as_mut().unwrap().token =
                            system_protocol::PendId::new(token.get() as u64);
                        lease.state = State::Granted;
                    }
                    Err(Fail::Pending) => (),
                    Err(failure) => {
                        lease.failure = Some(failure);
                        lease.state = State::Releasing;
                    }
                }
            }
            State::Granted => {
                let mut bytes = [0; PendInfo::SIZE];
                match pend::inspect(
                    lease.root.unwrap(),
                    VirtAddr::new(bytes.as_mut_ptr().addr()),
                ) {
                    Ok(())
                        if PendInfo::from_bytes(&bytes)
                            .is_some_and(|info| info.status == PendStatus::Active) => {}
                    Err(error) if error.source.is_busy() => (),
                    _ => {
                        lease.failure = Some(Fail::SourceLost);
                        lease.state = State::Releasing;
                    }
                }
            }
            State::Releasing => {
                if let Some(root) = lease.root {
                    if !retire(root)? {
                        return Ok(());
                    }
                    lease.root = None;
                }
                lease.delivery = None;
                lease.state = State::Released;
            }
            _ => (),
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<bool, Fail> {
        self.closing = true;
        for lease in &mut self.leases {
            if lease.state != State::Released {
                lease.state = State::Releasing;
            }
        }
        if self
            .leases
            .iter()
            .any(|lease| lease.state != State::Released)
        {
            self.advance()?;
            return Ok(false);
        }
        for definition in &mut self.definitions {
            if let Some(root) = definition.root {
                if !retire(root)? {
                    return Ok(false);
                }
                definition.root = None;
                return Ok(false);
            }
        }
        Ok(true)
    }
}
fn reply(lease: &Lease) -> Result<api::Reply, Fail> {
    if let Some(failure) = lease.failure {
        return Err(failure);
    }
    Ok(api::Reply {
        state: lease.state,
        delivery: Optional(if lease.state == State::Granted {
            lease.delivery.clone()
        } else {
            None
        }),
    })
}
fn valid_key(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control)
}
fn accord(
    source: PendId,
    target: AccordTarget,
    mark: Mark,
    limits: Limits,
) -> Result<PendId, Fail> {
    let request = AccordRequest {
        src: source,
        target,
        mark,
        limits,
    }
    .to_bytes();
    pend::accord(VirtAddr::new(request.as_ptr().addr())).map_err(|error| failure(error.source))
}
fn retire(token: PendId) -> Result<bool, Fail> {
    match pend::return_(token) {
        Ok(()) => Ok(true),
        Err(error) if error.source.is_busy() => Ok(false),
        Err(error) => {
            // An ancestor can already have retired this private root. It was
            // never transferred; a missing local record proves Kernel removed
            // it only after completing descendant withdrawal.
            let mut bytes = [0; PendInfo::SIZE];
            match pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())) {
                Err(inspect) if inspect.source == PendFail::Denied => Ok(true),
                _ => Err(failure(error.source)),
            }
        }
    }
}
fn failure(error: PendFail) -> Fail {
    match error {
        PendFail::Busy => Fail::Pending,
        PendFail::OutOfMemory => Fail::Full,
        PendFail::Denied | PendFail::Dead | PendFail::HandedOver => Fail::SourceLost,
        PendFail::Invalid | PendFail::NotAligned => Fail::Bad,
    }
}

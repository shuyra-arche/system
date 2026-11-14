//! The initial Task's live native service connections and completion boundary.
use super::boot::Workers;
use crate::runtime::{heap, memory::next_request};
use kernel_api::{
    address::VirtAddr,
    model::{
        identity::{PendId, TaskId},
        pend::{AccordRequest, AccordTarget, Limits, PendInfo, Target},
        wait::Wait,
    },
};
use kernel_client::call::{pend, room, unit};
use system_protocol::{identity as id, operator as op};
use weave::{Codec, Contract, Operation};

pub(crate) const LIMIT_NS: u64 = 5_000_000_000;
pub(crate) struct Services {
    pub license: PendId,
    pub context: system_client::Context,
    pub caller: TaskId,
    workers: Workers,
}
impl Services {
    /// # Safety
    /// `space` is the initial Task's actual Internal Nest from its boot ledger.
    pub unsafe fn new(license: PendId, space: PendId) -> Self {
        let Target::Nest(nest) = inspect(space).target else {
            fatal("system: boot Space kind")
        };
        let context = system_client::Context::new(unsafe {
            twine::view::memory::Native::new(space, nest, next_request as fn() -> _)
        });
        let caller = match observe(kernel_api::model::unit::UnitQuery::Caller).target {
            kernel_api::model::unit::UnitTarget::Task(task) => task,
            _ => fatal("system: initial Task identity"),
        };
        let workers = Workers::new(license, context.clone())
            .unwrap_or_else(|_| fatal("system: service bootstrap ownership"));
        Self {
            license,
            context,
            caller,
            workers,
        }
    }
    pub fn start(&mut self) {
        let deadline = room::clock().saturating_add(LIMIT_NS);
        loop {
            heap::poll();
            match self.workers.advance() {
                Ok(true) => {
                    if self.workers.phase() != super::boot::Phase::Ready {
                        fatal("system: service bootstrap phase")
                    }
                    let publication = self.workers.publication().unwrap();
                    if inspect(publication.setup).owner != publication.operator
                        || publication.setup == publication.authority
                        || publication.setup == publication.discovery
                    {
                        fatal("system: invalid setup publication")
                    }
                    return;
                }
                Ok(false) => (),
                Err(_) => fatal("system: native service bootstrap failed"),
            }
            bound(deadline, "system: native service bootstrap timeout");
        }
    }
    pub fn health(&mut self) {
        if self.workers.advance().is_err() {
            fatal("system: native service lost")
        }
        if self.workers.revision().is_err() {
            fatal("system: identity revision lost")
        }
    }
    pub fn operator(&self) -> TaskId {
        self.workers.publication().unwrap().operator
    }
    pub fn identity_task(&self) -> TaskId {
        self.workers.publication().unwrap().identity
    }
    pub fn identity_entry(&self, grant: id::Grant) -> PendId {
        self.workers
            .identity_face(grant)
            .unwrap_or_else(|| fatal("system: identity endpoint unavailable"))
    }
    pub fn discovery(&self) -> PendId {
        self.workers.publication().unwrap().discovery
    }
    pub fn authority(&self) -> PendId {
        self.workers.publication().unwrap().authority
    }
    pub fn identity<O: Operation<Interface = id::operations::Interface>>(
        &self,
        grant: id::Grant,
        body: O::Request,
    ) -> <<O as Contract>::Response as Codec>::Decoded {
        let mut client = system_client::identity::Client::bind(
            self.identity_entry(grant),
            self.identity_task(),
            self.license,
            self.context.clone(),
        )
        .unwrap_or_else(|_| fatal("system: identity binding failed"));
        complete(client.begin::<O>(body, Wait::At(room::clock().saturating_add(LIMIT_NS))))
    }
    pub fn mutate<O: Operation<Interface = op::authority::operations::Interface>>(
        &self,
        body: O::Request,
    ) -> <<O as Contract>::Response as Codec>::Decoded {
        let mut client = system_client::authority::Client::bind(
            self.authority(),
            self.operator(),
            self.license,
            self.context.clone(),
        )
        .unwrap_or_else(|_| fatal("system: authority binding failed"));
        complete(client.begin::<O>(body, Wait::At(room::clock().saturating_add(LIMIT_NS))))
    }
    pub fn query<O: Operation<Interface = op::operations::Interface>>(
        &self,
        body: O::Request,
    ) -> <<O as Contract>::Response as Codec>::Decoded {
        let mut client = system_client::operator::Client::bind(
            self.discovery(),
            self.operator(),
            self.license,
            self.context.clone(),
        )
        .unwrap_or_else(|_| fatal("system: discovery binding failed"));
        complete(client.begin::<O>(body, Wait::At(room::clock().saturating_add(LIMIT_NS))))
    }
    pub fn finish(&mut self) {
        let deadline = room::clock().saturating_add(LIMIT_NS * 2);
        loop {
            heap::poll();
            match self.workers.finish() {
                Ok(true) => {
                    if self.workers.observations().iter().any(|info| {
                        info.is_none_or(|info| {
                            !info.reclaimed
                                || info.result != Some(0)
                                || info.exit.is_none_or(|(_, reason)| reason != 0)
                        })
                    }) {
                        fatal("system: service reclamation unproven")
                    }
                    return;
                }
                Ok(false) => (),
                Err(_) => fatal("system: native service retirement failed"),
            }
            bound(deadline, "system: native service retirement timeout");
        }
    }
}

/// Submit once. A failed delivery never triggers a second service effect.
/// Unsettled transport/cleanup aborts this Team with all owners still present;
/// Kernel owns physical retirement, rather than a fabricated successful reply.
pub(crate) fn complete<O: Operation>(
    call: Result<system_client::Pending<O>, system_client::Failure>,
) -> <<O as Contract>::Response as Codec>::Decoded {
    let mut call = match call {
        Ok(call) => call,
        Err(_) => fatal("system: service request submission failed"),
    };
    let deadline = room::clock().saturating_add(LIMIT_NS);
    let response = loop {
        heap::poll();
        match call.poll() {
            Ok(Some(response)) => break response.value,
            Ok(None) => (),
            Err(_) => fatal("system: service response unsettled"),
        }
        bound(deadline, "system: service response timed out");
    };
    let mut cleanup = call.into_cleanup();
    let deadline = room::clock().saturating_add(LIMIT_NS);
    while cleanup.finish().is_err() {
        heap::poll();
        bound(deadline, "system: service reply retirement timed out");
    }
    response
}
pub(crate) fn inspect(token: PendId) -> PendInfo {
    let mut bytes = [0; PendInfo::SIZE];
    let deadline = room::clock().saturating_add(LIMIT_NS);
    loop {
        match pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())) {
            Ok(()) => {
                return PendInfo::from_bytes(&bytes)
                    .unwrap_or_else(|| fatal("system: malformed Pend observation"));
            }
            Err(error) if error.source.is_busy() => {
                bound(deadline, "system: Pend observation timeout")
            }
            Err(_) => fatal("system: native Pend authority lost"),
        }
    }
}
pub(crate) fn observe(
    query: kernel_api::model::unit::UnitQuery,
) -> kernel_api::model::unit::UnitInfo {
    let mut bytes = [0; kernel_api::model::unit::UnitInfo::WIDTH];
    let deadline = room::clock().saturating_add(LIMIT_NS);
    loop {
        match unit::inspect(query, VirtAddr::new(bytes.as_mut_ptr().addr())) {
            Ok(true) => {
                return kernel_api::model::unit::UnitInfo::from_bytes(&bytes)
                    .unwrap_or_else(|| fatal("system: malformed Unit observation"));
            }
            Err(error) if error.source.is_busy() => {
                bound(deadline, "system: Unit observation timeout")
            }
            _ => fatal("system: Unit observation lost"),
        }
    }
}
pub(crate) fn give(
    source: PendId,
    target: TaskId,
    mark: kernel_api::model::identity::Mark,
    limits: Limits,
) -> Result<PendId, kernel_api::abi::call::pend::PendFail> {
    let bytes = AccordRequest {
        src: source,
        target: AccordTarget::Task(target),
        mark,
        limits,
    }
    .to_bytes();
    pend::accord(VirtAddr::new(bytes.as_ptr().addr())).map_err(|error| error.source)
}
pub(crate) fn release(token: PendId) -> bool {
    match pend::return_(token) {
        Ok(()) => true,
        Err(error) if error.source.is_busy() => false,
        Err(_) => fatal("system: owned Pend retirement failed"),
    }
}
pub(crate) fn bound(deadline: u64, note: &'static str) {
    if room::clock() >= deadline {
        fatal(note)
    }
    room::yield_();
}
pub(crate) fn fatal(note: &'static str) -> ! {
    unit::exit(
        0,
        4,
        VirtAddr::new(note.as_ptr().addr()),
        note.len(),
        kernel_api::model::unit::ExitScope::Team,
    )
}

/// Untrusted coordinates and caller observations fail admission, not System.
pub(crate) fn probe(token: PendId) -> Option<PendInfo> {
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())).ok()?;
    PendInfo::from_bytes(&bytes)
}
pub(crate) fn lookup(task: TaskId) -> Option<kernel_api::model::unit::UnitInfo> {
    let mut bytes = [0; kernel_api::model::unit::UnitInfo::WIDTH];
    if !unit::inspect(
        kernel_api::model::unit::UnitQuery::Target(task.into()),
        VirtAddr::new(bytes.as_mut_ptr().addr()),
    )
    .ok()?
    {
        return None;
    }
    kernel_api::model::unit::UnitInfo::from_bytes(&bytes)
}

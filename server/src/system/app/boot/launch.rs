//! A supervisor-owned handoff stays allocated until the worker is reclaimed.
use super::Error;
use crate::system::{
    app::worker::{Phase, Worker},
    identity::{revision::Epoch, service as identity},
    operator::runtime as operator,
};
use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use kernel_api::{
    address::VirtAddr,
    model::{
        identity::{Mark, PendId, TaskId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, Distributable, Limits, Operations, PendRange, PendScope,
        },
        sign::SignRights,
        unit::ExitScope,
        wait::Wait,
    },
};
use kernel_client::call::{pend, room, sign, unit};

#[derive(Clone, Copy)]
pub(super) enum Role {
    Operator,
    Identity,
}

pub(super) struct Handoff {
    role: Role,
    pub controller: TaskId,
    pub operator: AtomicUsize,
    pub epoch: Epoch,
    pub grants: [AtomicUsize; 5],
    pub publications: [AtomicUsize; 20],
    pub health: AtomicUsize,
    armed: AtomicBool,
    pub ready: AtomicBool,
    pub failed: AtomicBool,
}

pub(super) struct Slot {
    pub worker: Worker,
    handoff: Option<Box<Handoff>>,
}
impl Slot {
    pub fn prepare(role: Role, controller: TaskId, epoch: Epoch) -> Result<Self, Error> {
        let handoff = Box::new(Handoff {
            role,
            controller,
            operator: AtomicUsize::new(0),
            epoch,
            grants: [const { AtomicUsize::new(0) }; 5],
            publications: [const { AtomicUsize::new(0) }; 20],
            health: AtomicUsize::new(0),
            armed: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        });
        let words = [(&*handoff as *const Handoff).addr()];
        // SAFETY: Slot retains this stable allocation until Kernel reclamation.
        // Worker writes only atomics; Epoch contains a shared atomic revision.
        let worker =
            unsafe { Worker::prepare(entry, &words, 256 * 1024) }.map_err(Error::Worker)?;
        Ok(Self {
            worker,
            handoff: Some(handoff),
        })
    }
    pub fn handoff(&self) -> &Handoff {
        self.handoff.as_ref().expect("owned handoff")
    }
    pub fn grant(
        &self,
        source: PendId,
        index: usize,
        operations: Operations,
        modifiable: Modifiable,
    ) -> Result<(), Error> {
        if self.handoff().grants[index].load(Ordering::Acquire) != 0 {
            return Ok(());
        }
        let request = AccordRequest {
            src: source,
            target: AccordTarget::Task(self.worker.task()),
            mark: Mark::NONE,
            limits: Limits {
                operations,
                modifiable,
                distributable: Distributable::No,
                scope: match operations {
                    Operations::Share(_) | Operations::Flat(_) => {
                        PendScope::Pages(PendRange::Inherit)
                    }
                    Operations::Nest(_) => {
                        PendScope::Nest(kernel_api::model::pend::NestScope::Whole)
                    }
                    _ => PendScope::Object,
                },
            },
        }
        .to_bytes();
        let delivered =
            pend::accord(VirtAddr::new(request.as_ptr().addr())).map_err(Error::Pend)?;
        // This coordinate belongs to the recipient; never use it in this Task.
        self.handoff().grants[index].store(delivered.get(), Ordering::Release);
        Ok(())
    }
    pub fn resume(&mut self) -> Result<(), Error> {
        self.handoff().armed.store(true, Ordering::Release);
        self.worker.resume().map_err(Error::Worker)
    }
    pub fn ready(&mut self) -> Result<bool, Error> {
        if self.handoff().failed.load(Ordering::Acquire) {
            return Err(Error::WorkerFailed);
        }
        if self.worker.observe().map_err(Error::Worker)?.state
            == kernel_api::model::unit::UnitState::Exited
        {
            return Err(Error::WorkerFailed);
        }
        Ok(self.handoff().ready.load(Ordering::Acquire))
    }
    pub fn finish(&mut self) -> Result<bool, Error> {
        self.worker.finish().map_err(Error::Worker)
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        if self.worker.phase() != Phase::Closed {
            let _ = self.worker.stop();
            if !matches!(self.worker.finish(), Ok(true)) {
                // An incomplete owner must not free storage that another Hart
                // can still read. Explicit finish avoids this conservative leak.
                if let Some(handoff) = self.handoff.take() {
                    Box::leak(handoff);
                }
            }
        }
    }
}
fn native(raw: usize) -> PendId {
    PendId::from_bytes(&(raw as u64).to_le_bytes()).expect("native handoff word")
}
fn signal_health(license: PendId, controller: TaskId) -> Result<(PendId, PendId), Error> {
    let root = sign::create(license).map_err(Error::Sign)?;
    let request = AccordRequest {
        src: root,
        target: AccordTarget::Task(controller),
        mark: Mark::NONE,
        limits: Limits {
            operations: Operations::Sign(SignRights::LISTEN),
            modifiable: Modifiable::No,
            distributable: Distributable::Yes,
            scope: PendScope::Object,
        },
    }
    .to_bytes();
    loop {
        match pend::accord(VirtAddr::new(request.as_ptr().addr())) {
            Ok(delivered) => return Ok((root, delivered)),
            Err(error) if error.source.is_busy() => room::yield_(),
            Err(error) => {
                while pend::return_(root).is_err() {
                    room::yield_();
                }
                return Err(Error::Pend(error));
            }
        }
    }
}
fn operator(handoff: &Handoff) -> bool {
    let license = native(handoff.grants[0].load(Ordering::Acquire));
    let (health, published) = match signal_health(license, handoff.controller) {
        Ok(value) => value,
        Err(_) => return false,
    };
    handoff.health.store(published.get(), Ordering::Release);
    let Some(context) = context(handoff) else {
        return false;
    };
    let config = operator::Config {
        context,
        license,
        controller: handoff.controller,
        shutdown: native(handoff.grants[1].load(Ordering::Acquire)),
    };
    let mut service = match operator::Service::create(config) {
        Ok(value) => value,
        Err(mut failure) => {
            while failure.service.finish().is_err() {
                room::yield_();
            }
            while pend::return_(health).is_err() {
                room::yield_();
            }
            return false;
        }
    };
    let published = service
        .publication()
        .expect("initialized Operator publication");
    for (index, id) in [published.discovery, published.authority, published.setup]
        .into_iter()
        .enumerate()
    {
        handoff.publications[index].store(id.get() as usize, Ordering::Release);
    }
    handoff.ready.store(true, Ordering::Release);
    let success = loop {
        match service.step(Wait::Infinite) {
            Ok(report) if report.stopped => break true,
            Ok(_) => (),
            Err(_) => break false,
        }
    };
    while service.finish().is_err() {
        room::yield_();
    }
    while pend::return_(health).is_err() {
        room::yield_();
    }
    success
}
fn identity(handoff: &Handoff) -> bool {
    let Some(context) = context(handoff) else {
        return false;
    };
    let config = identity::Config {
        context,
        license: native(handoff.grants[0].load(Ordering::Acquire)),
        controller: handoff.controller,
        operator: TaskId::new(handoff.operator.load(Ordering::Acquire)),
        shutdown: native(handoff.grants[1].load(Ordering::Acquire)),
        notify: native(handoff.grants[2].load(Ordering::Acquire)),
        dependency: native(handoff.grants[3].load(Ordering::Acquire)),
        epoch: handoff.epoch.clone(),
    };
    let mut service = match identity::Service::create(config) {
        Ok(value) => value,
        Err(mut failure) => {
            while failure.service.finish().is_err() {
                room::yield_();
            }
            return false;
        }
    };
    for publication in service.publications() {
        handoff.publications[publication.grant as usize - 1]
            .store(publication.token.get() as usize, Ordering::Release);
    }
    handoff.ready.store(true, Ordering::Release);
    let success = loop {
        match service.step(Wait::Infinite) {
            Ok(report) if report.stopped => break true,
            Ok(_) => (),
            Err(_) => break false,
        }
    };
    while service.finish().is_err() {
        room::yield_();
    }
    success
}
unsafe extern "C" fn entry(words: *const usize, count: usize) -> ! {
    if count != 1 {
        unit::exit(1, 0, VirtAddr::new(0), 0, ExitScope::Task);
    }
    // SAFETY: Kernel provides the copied stack words; Slot retains their one
    // pointed-to handoff until this Task's independent reclaim observation.
    let handoff = unsafe { &*(*words as *const Handoff) };
    while !handoff.armed.load(Ordering::Acquire) {
        room::yield_();
    }
    let success = match handoff.role {
        Role::Operator => operator(handoff),
        Role::Identity => identity(handoff),
    };
    if !success {
        handoff.failed.store(true, Ordering::Release);
    }
    unit::exit(u64::from(!success), 0, VirtAddr::new(0), 0, ExitScope::Task)
}

fn context(handoff: &Handoff) -> Option<system_client::Context> {
    let space = native(handoff.grants[4].load(Ordering::Acquire));
    let mut bytes = [0; kernel_api::model::pend::PendInfo::SIZE];
    let deadline = room::clock().saturating_add(super::BOOT_NS);
    loop {
        match pend::inspect(space, VirtAddr::new(bytes.as_mut_ptr().addr())) {
            Ok(()) => break,
            Err(error) if error.source.is_busy() && room::clock() < deadline => room::yield_(),
            Err(_) => return None,
        }
    }
    let info = kernel_api::model::pend::PendInfo::from_bytes(&bytes)?;
    let kernel_api::model::pend::Target::Nest(nest) = info.target else {
        return None;
    };
    // SAFETY: the trusted same-Team controller explicitly delivered its own
    // Internal Nest. This context remains in this worker, shares the System
    // Team's monotonic allocator and is retained by every ingress and query.
    let memory = unsafe {
        twine::view::memory::Native::new(
            space,
            nest,
            crate::runtime::memory::next_request as fn() -> _,
        )
    };
    Some(system_client::Context::new(memory))
}

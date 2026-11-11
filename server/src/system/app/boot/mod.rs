//! Native Operator/Identity bootstrap and independently observed worker retirement.
mod launch;
use crate::system::{
    app::worker,
    identity::revision::{self, Epoch},
};
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use kernel_api::{
    abi::call::{pend::PendFail, sign::SignFail, unit::UnitFail},
    address::VirtAddr,
    model::{
        identity::{PendId, TaskId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, CloneRights, CreationRights, Distributable, Limits,
            Operations, PendInfo, PendRange,
        },
        sign::SignRights,
        unit::{UnitInfo, UnitQuery, UnitTarget},
        wait::Wait,
    },
};
use kernel_client::call::{pend, room, sign, unit};
use launch::{Role, Slot};
use system_client::{Cleanup, Pending, setup::Client};
use system_protocol::{
    identity::Grant,
    operator::{self as op, setup::operations as setup},
    value::Outcome,
};

const BOOT_NS: u64 = 5_000_000_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Preparing,
    Operator,
    Identity,
    Root,
    Wiring,
    Ready,
    Closing,
    Closed,
    Failed,
}
#[derive(Debug)]
pub enum Error {
    Pend(erra::Error<PendFail>),
    Sign(erra::Error<SignFail>),
    Unit(erra::Error<UnitFail>),
    Worker(worker::Error),
    WorkerFailed,
    Invalid,
    Capacity,
    TimedOut,
    Phase,
    Client(system_client::Error),
    Cleanup(system_client::CleanupError),
    Operator(op::Fail),
    Revision(revision::Error),
}
impl Error {
    fn busy(&self) -> bool {
        match self {
            Self::Pend(e) => e.source.is_busy(),
            Self::Sign(e) => e.source.is_busy(),
            Self::Unit(e) => e.source.is_busy(),
            Self::Worker(worker::Error::Kernel(e)) => e.source.is_busy(),
            _ => false,
        }
    }
}

/// Caller-local publications, available only after Root and Wired are acknowledged.
#[derive(Clone, Copy, Debug)]
pub struct Publication {
    pub operator: TaskId,
    pub identity: TaskId,
    pub discovery: PendId,
    pub authority: PendId,
    pub setup: PendId,
}

#[must_use = "finish retained bootstrap resources, including after advance fails"]
pub struct Workers {
    license: PendId,
    controller: TaskId,
    epoch: Epoch,
    phase: Phase,
    deadline: u64,
    signals: [Option<PendId>; 3],
    operator: Option<Slot>,
    identity: Option<Slot>,
    // These coordinates belong to the Operator table, not this controller.
    wired: [Option<system_protocol::PendId>; 3],
    root: Option<Pending<setup::Root>>,
    wiring: Option<Pending<setup::Wired>>,
    cleanup: Option<Cleanup>,
    retiring: Vec<PendId>,
    shutdown: Option<u64>,
    notified: [bool; 2],
}
impl Workers {
    /// Borrow the initial Task's creation license; never discover grants by Mark.
    pub fn new(license: PendId) -> Result<Self, Error> {
        let mut bytes = [0; UnitInfo::WIDTH];
        if !unit::inspect(UnitQuery::Caller, VirtAddr::new(bytes.as_mut_ptr().addr()))
            .map_err(Error::Unit)?
        {
            return Err(Error::Invalid);
        }
        let UnitTarget::Task(controller) =
            UnitInfo::from_bytes(&bytes).ok_or(Error::Invalid)?.target
        else {
            return Err(Error::Invalid);
        };
        let mut retiring = Vec::new();
        retiring
            .try_reserve_exact(24)
            .map_err(|_| Error::Capacity)?;
        Ok(Self {
            license,
            controller,
            epoch: Epoch::new(),
            phase: Phase::Preparing,
            deadline: room::clock().saturating_add(BOOT_NS),
            signals: [None; 3],
            operator: None,
            identity: None,
            wired: [None; 3],
            root: None,
            wiring: None,
            cleanup: None,
            retiring,
            shutdown: None,
            notified: [false; 2],
        })
    }
    pub fn observations(&self) -> [Option<&UnitInfo>; 2] {
        [
            self.operator.as_ref().and_then(|slot| slot.worker.result()),
            self.identity.as_ref().and_then(|slot| slot.worker.result()),
        ]
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn epoch(&self) -> &Epoch {
        &self.epoch
    }
    pub fn changed(&self) -> Option<PendId> {
        self.signals[2]
    }
    pub fn revision(&self) -> Result<u64, Error> {
        revision::consume(self.changed().ok_or(Error::Phase)?, &self.epoch).map_err(Error::Revision)
    }
    pub fn publication(&self) -> Option<Publication> {
        if self.phase != Phase::Ready || self.cleanup.is_some() {
            return None;
        }
        let slot = self.operator.as_ref()?;
        let endpoint = |i: usize| native(slot.handoff().publications[i].load(Ordering::Acquire));
        Some(Publication {
            operator: slot.worker.task(),
            identity: self.identity.as_ref()?.worker.task(),
            discovery: endpoint(0),
            authority: endpoint(1),
            setup: endpoint(2),
        })
    }
    pub fn identity_face(&self, grant: Grant) -> Option<PendId> {
        if self.phase != Phase::Ready || self.cleanup.is_some() {
            return None;
        }
        Some(native(
            self.identity.as_ref()?.handoff().publications[grant as usize - 1]
                .load(Ordering::Acquire),
        ))
    }
    /// One bounded supervisor round. Busy preserves acquired objects; RPCs are
    /// submitted once, then polled without repeating Root or Wired side effects.
    pub fn advance(&mut self) -> Result<bool, Error> {
        if matches!(self.phase, Phase::Closing | Phase::Closed | Phase::Failed) {
            return Err(Error::Phase);
        }
        let result = self.progress();
        match result {
            Err(error) if error.busy() => Ok(false),
            Err(error) => {
                self.phase = Phase::Failed;
                Err(error)
            }
            other => other,
        }
    }
    fn progress(&mut self) -> Result<bool, Error> {
        if (self.phase != Phase::Ready || self.cleanup.is_some()) && room::clock() >= self.deadline
        {
            return Err(Error::TimedOut);
        }
        if let Some(cleanup) = self.cleanup.as_mut() {
            if cleanup.finish().is_err() {
                return Ok(false);
            }
            self.cleanup = None;
        }
        if self.phase == Phase::Ready {
            self.operator.as_mut().unwrap().ready()?;
            self.identity.as_mut().unwrap().ready()?;
            return Ok(true);
        }
        if room::clock() >= self.deadline {
            return Err(Error::TimedOut);
        }
        match self.phase {
            Phase::Preparing => {
                for signal in &mut self.signals {
                    if signal.is_none() {
                        *signal = Some(sign::create(self.license).map_err(Error::Sign)?);
                        return Ok(false);
                    }
                }
                if self.operator.is_none() {
                    self.operator = Some(Slot::prepare(
                        Role::Operator,
                        self.controller,
                        self.epoch.clone(),
                    )?);
                }
                let slot = self.operator.as_ref().unwrap();
                slot.grant(
                    self.license,
                    0,
                    Operations::None(CreationRights::CLONE | CreationRights::SIGN),
                    Modifiable::No,
                )?;
                slot.grant(
                    self.signals[0].unwrap(),
                    1,
                    Operations::Sign(SignRights::LISTEN | SignRights::CONSUME | SignRights::ROUTE),
                    Modifiable::No,
                )?;
                self.operator.as_mut().unwrap().resume()?;
                self.phase = Phase::Operator;
            }
            Phase::Operator => {
                if !self.operator.as_mut().unwrap().ready()? {
                    return Ok(false);
                }
                if self.identity.is_none() {
                    self.identity = Some(Slot::prepare(
                        Role::Identity,
                        self.controller,
                        self.epoch.clone(),
                    )?);
                }
                let slot = self.identity.as_ref().unwrap();
                slot.handoff().operator.store(
                    self.operator.as_ref().unwrap().worker.task().get(),
                    Ordering::Release,
                );
                slot.grant(
                    self.license,
                    0,
                    Operations::None(CreationRights::CLONE | CreationRights::SIGN),
                    Modifiable::No,
                )?;
                slot.grant(
                    self.signals[1].unwrap(),
                    1,
                    Operations::Sign(SignRights::LISTEN | SignRights::CONSUME | SignRights::ROUTE),
                    Modifiable::No,
                )?;
                slot.grant(
                    self.signals[2].unwrap(),
                    2,
                    Operations::Sign(SignRights::TRIGGER),
                    Modifiable::No,
                )?;
                let health = native(
                    self.operator
                        .as_ref()
                        .unwrap()
                        .handoff()
                        .health
                        .load(Ordering::Acquire),
                );
                slot.grant(
                    health,
                    3,
                    Operations::Sign(SignRights::LISTEN),
                    Modifiable::No,
                )?;
                self.identity.as_mut().unwrap().resume()?;
                self.phase = Phase::Identity;
            }
            Phase::Identity => {
                if !self.identity.as_mut().unwrap().ready()? {
                    return Ok(false);
                }
                let slot = self.operator.as_ref().unwrap();
                let endpoint = native(slot.handoff().publications[2].load(Ordering::Acquire));
                let mut setup = Client::bind(endpoint, slot.worker.task(), self.license)
                    .map_err(Error::Client)?;
                match setup.begin::<setup::Root>((), Wait::At(self.deadline)) {
                    Ok(call) => self.root = Some(call),
                    Err(failure) => {
                        self.cleanup = Some(failure.cleanup);
                        return Err(Error::Client(failure.error));
                    }
                }
                self.phase = Phase::Root;
            }
            Phase::Root => {
                let Some(response) = self.root.as_mut().unwrap().poll().map_err(Error::Client)?
                else {
                    return Ok(false);
                };
                self.cleanup = Some(self.root.take().unwrap().into_cleanup());
                if let Outcome::Failure(why) = response.value {
                    return Err(Error::Operator(why));
                }
                self.phase = Phase::Wiring;
            }
            Phase::Wiring => {
                if self.wiring.is_none() {
                    let identity = self.identity.as_ref().unwrap();
                    let operator = self.operator.as_ref().unwrap();
                    for (index, grant) in [Grant::Resolve, Grant::Matches, Grant::Same]
                        .into_iter()
                        .enumerate()
                    {
                        if self.wired[index].is_some() {
                            continue;
                        }
                        let src = native(
                            identity.handoff().publications[grant as usize - 1]
                                .load(Ordering::Acquire),
                        );
                        let request = AccordRequest {
                            src,
                            target: AccordTarget::Task(operator.worker.task()),
                            mark: kernel_api::model::identity::Mark::NONE,
                            limits: Limits {
                                operations: Operations::Clone(
                                    CloneRights::PUSH | CloneRights::PEEK,
                                ),
                                modifiable: Modifiable::Yes,
                                distributable: Distributable::No,
                                range: PendRange::Inherit,
                            },
                        }
                        .to_bytes();
                        self.wired[index] = Some(
                            pend::accord(VirtAddr::new(request.as_ptr().addr()))
                                .map_err(Error::Pend)?
                                .into(),
                        );
                        return Ok(false);
                    }
                    let body = op::Wired {
                        authority: identity.worker.task().into(),
                        resolve: self.wired[0].unwrap(),
                        matches: self.wired[1].unwrap(),
                        same: self.wired[2].unwrap(),
                    };
                    let endpoint =
                        native(operator.handoff().publications[2].load(Ordering::Acquire));
                    let mut setup = Client::bind(endpoint, operator.worker.task(), self.license)
                        .map_err(Error::Client)?;
                    match setup.begin::<setup::Wired>(body, Wait::At(self.deadline)) {
                        Ok(call) => self.wiring = Some(call),
                        Err(failure) => {
                            self.cleanup = Some(failure.cleanup);
                            return Err(Error::Client(failure.error));
                        }
                    }
                    return Ok(false);
                }
                let Some(response) = self
                    .wiring
                    .as_mut()
                    .unwrap()
                    .poll()
                    .map_err(Error::Client)?
                else {
                    return Ok(false);
                };
                self.cleanup = Some(self.wiring.take().unwrap().into_cleanup());
                if let Outcome::Failure(why) = response.value {
                    return Err(Error::Operator(why));
                }
                self.phase = Phase::Ready;
            }
            _ => return Err(Error::Phase),
        }
        Ok(false)
    }
    /// Stop both roles, then retain handoffs/publications until Kernel confirms
    /// reclamation. A grace deadline permits forced stop if cleanup cannot run.
    pub fn finish(&mut self) -> Result<bool, Error> {
        if self.phase == Phase::Closed {
            return Ok(true);
        }
        self.phase = Phase::Closing;
        let deadline = *self
            .shutdown
            .get_or_insert_with(|| room::clock().saturating_add(BOOT_NS));
        // Identity borrows Operator-owned authority. Retire the dependent first,
        // keeping Operator alive until Identity has relinquished its endpoints.
        for (index, slot) in [(1, &mut self.identity), (0, &mut self.operator)] {
            if !self.notified[index] {
                if let Some(signal) = self.signals[index] {
                    match sign::trigger(signal) {
                        Ok(()) => (),
                        Err(error) if error.source.is_busy() => return Ok(false),
                        Err(error) => return Err(Error::Sign(error)),
                    }
                }
                self.notified[index] = true;
            }
            let Some(slot) = slot.as_mut() else {
                continue;
            };
            if slot.worker.phase() == worker::Phase::Prepared
                || room::clock() >= deadline && slot.worker.phase() == worker::Phase::Started
            {
                slot.worker.stop().map_err(Error::Worker)?;
            }
            match slot.finish() {
                Ok(true) => (),
                Ok(false) => return Ok(false),
                Err(error) if error.busy() => return Ok(false),
                Err(error) => return Err(error),
            }
        }
        if let Some(call) = self.root.take() {
            self.cleanup = Some(call.into_cleanup());
        }
        if let Some(call) = self.wiring.take() {
            self.cleanup = Some(call.into_cleanup());
        }
        if let Some(cleanup) = self.cleanup.as_mut() {
            cleanup.finish().map_err(Error::Cleanup)?;
            self.cleanup = None;
        }
        if let Some(slot) = self.identity.as_ref() {
            for value in &slot.handoff().publications {
                let raw = value.swap(0, Ordering::AcqRel);
                if raw != 0 {
                    self.retiring.push(native(raw));
                }
            }
        }
        if let Some(slot) = self.operator.as_ref() {
            for value in &slot.handoff().publications[..3] {
                let raw = value.swap(0, Ordering::AcqRel);
                if raw != 0 {
                    self.retiring.push(native(raw));
                }
            }
            let raw = slot.handoff().health.swap(0, Ordering::AcqRel);
            if raw != 0 {
                self.retiring.push(native(raw));
            }
        }
        while let Some(token) = self.retiring.last().copied() {
            release(token)?;
            self.retiring.pop();
        }
        for signal in &mut self.signals {
            if let Some(token) = *signal {
                release(token)?;
                *signal = None;
            }
        }
        self.phase = Phase::Closed;
        Ok(true)
    }
}
fn native(raw: usize) -> PendId {
    PendId::from_bytes(&(raw as u64).to_le_bytes()).expect("native recipient coordinate")
}
fn release(token: PendId) -> Result<(), Error> {
    match pend::return_(token) {
        Ok(()) => Ok(()),
        Err(error) if !error.source.is_busy() => {
            let mut bytes = [0; PendInfo::SIZE];
            match pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())) {
                Err(probe) if !probe.source.is_busy() => Ok(()),
                _ => Err(Error::Pend(error)),
            }
        }
        Err(error) => Err(Error::Pend(error)),
    }
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Pend(error) => write!(f, "bootstrap Pend: {error}"),
            Self::Sign(error) => write!(f, "bootstrap Sign: {error}"),
            Self::Unit(error) => write!(f, "bootstrap Unit: {error}"),
            Self::Worker(error) => write!(f, "bootstrap worker: {error:?}"),
            Self::Client(error) => write!(f, "bootstrap call: {error:?}"),
            Self::Cleanup(error) => write!(f, "bootstrap cleanup: {error:?}"),
            Self::Operator(error) => write!(f, "bootstrap Operator: {error:?}"),
            Self::Revision(error) => write!(f, "bootstrap revision: {error:?}"),
            Self::WorkerFailed => f.write_str("bootstrap worker exited or failed"),
            Self::Invalid => f.write_str("invalid bootstrap caller"),
            Self::Capacity => f.write_str("bootstrap capacity exhausted"),
            Self::TimedOut => f.write_str("bootstrap deadline elapsed"),
            Self::Phase => f.write_str("invalid bootstrap phase"),
        }
    }
}

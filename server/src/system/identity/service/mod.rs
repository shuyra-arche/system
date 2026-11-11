//! Identity's bounded multi-endpoint Task loop on Kernel Clone and Sign.
pub(super) mod answer;
mod face;
use super::revision;
pub mod run;

use alloc::vec::Vec;
use core::fmt;
use erra::Error as KernelError;
use kernel_api::{
    abi::call::{pend::PendFail, sign::SignFail, unit::UnitFail},
    address::VirtAddr,
    model::{
        identity::{PendId, TaskId},
        pend::{PendInfo, PendStatus},
        sign::SignSource,
        wait::Wait,
    },
};
use kernel_client::call::{pend, unit};
use system_protocol::identity::Grant;

pub use super::revision::Epoch;
use crate::system::app::waiting::{Interest, Waiting};
use crate::system::identity::book::IdentityBook;
use face::Face;
pub use face::Publication;

/// Borrowed Pend authorities in the serving Task's own table. The caller owns
/// these grants and keeps them alive until Service::finish completes.
pub struct Config {
    pub license: PendId,
    pub controller: TaskId,
    pub shutdown: PendId,
    pub operator: TaskId,
    pub dependency: PendId,
    pub notify: PendId,
    pub epoch: Epoch,
}

#[derive(Debug)]
pub enum Error {
    InvalidConfig,
    Inspect(KernelError<PendFail>),
    Unit(KernelError<UnitFail>),
    Book(system_protocol::identity::Fail),
    Capacity,
    Face(face::Error),
    Sign(KernelError<SignFail>),
    Dependency,
    Revision(revision::Error),
    Ingress(crate::protocol::Error),
    Cleanup(CleanupError),
    Waiting(crate::system::app::waiting::Error),
    Inactive,
}

#[derive(Debug)]
pub enum CleanupError {
    Mux(crate::system::app::waiting::Error),
    Face(face::CleanupError),
}

enum Phase {
    Starting,
    Running,
    Closing,
    Closed,
}

/// Owns queues and event subscriptions. Failed construction or progress keeps
/// this object available for explicit cleanup. Drop is not a completion proof.
#[must_use = "finish the service and retry failed cleanup before leaving the Task"]
pub struct Service {
    config: Config,
    phase: Phase,
    task: TaskId,
    book: Option<IdentityBook>,
    faces: Vec<Face>,
    waiting: Option<Waiting>,
    revision: revision::Publisher,
    cursor: usize,
}

impl fmt::Debug for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Service")
            .field("task", &self.task)
            .field("faces", &self.faces.len())
            .field("closed", &matches!(self.phase, Phase::Closed))
            .finish()
    }
}

#[derive(Debug)]
#[must_use = "finish the retained service after a failed construction or run"]
pub struct Failure {
    pub error: Error,
    pub service: Service,
}

impl Service {
    #[allow(
        clippy::result_large_err,
        reason = "return acquired resources without allocating on failure"
    )]
    pub fn create(config: Config) -> Result<Self, Failure> {
        let revision = revision::Publisher::new(config.epoch.clone());
        let mut service = Self {
            config,
            phase: Phase::Starting,
            task: TaskId::new(0),
            book: None,
            faces: Vec::new(),
            waiting: None,
            revision,
            cursor: 0,
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
        use kernel_api::model::{
            pend::Operations,
            sign::SignRights,
            unit::{UnitInfo, UnitQuery, UnitTarget},
        };
        if self.config.controller.get() == 0 || self.config.operator.get() == 0 {
            return Err(Error::InvalidConfig);
        }
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
        if task == self.config.controller || task == self.config.operator {
            return Err(Error::InvalidConfig);
        }
        let shutdown = inspect(self.config.shutdown)?;
        let notify = inspect(self.config.notify)?;
        let dependency = inspect(self.config.dependency)?;
        if shutdown.owner != self.config.controller
            || notify.owner != self.config.controller
            || dependency.owner != self.config.operator
            || !matches!(shutdown.limits.operations, Operations::Sign(rights) if rights.contains(SignRights::LISTEN | SignRights::CONSUME))
            || !matches!(notify.limits.operations, Operations::Sign(rights) if rights.contains(SignRights::TRIGGER))
        {
            return Err(Error::InvalidConfig);
        }
        self.book = Some(
            IdentityBook::new(task.into(), self.config.controller.into()).map_err(Error::Book)?,
        );
        self.faces
            .try_reserve_exact(Grant::ALL.len())
            .map_err(|_| Error::Capacity)?;
        self.waiting = Some(Waiting::new(self.config.license).map_err(Error::Waiting)?);
        for grant in Grant::ALL {
            self.faces.push(Face::new(grant));
            self.faces
                .last_mut()
                .unwrap()
                .initialize(self.config.license, self.config.controller)
                .map_err(Error::Face)?;
        }
        let mut reads = Vec::new();
        reads
            .try_reserve_exact(self.faces.len())
            .map_err(|_| Error::Capacity)?;
        reads.extend(self.faces.iter().map(|face| face.root.unwrap()));
        let extra = [
            Interest::new(SignSource::Sign {
                token: self.config.shutdown,
            }),
            Interest::new(SignSource::Inspect {
                task,
                token: self.config.shutdown,
            }),
            Interest::new(SignSource::Inspect {
                task,
                token: self.config.dependency,
            }),
        ];
        self.waiting
            .as_mut()
            .unwrap()
            .apply(&reads, &[], &extra)
            .map_err(Error::Waiting)?;
        Ok(())
    }

    /// The identifiers are already local to the controller's table. Publish
    /// this receipt only after construction succeeds; never discover by Mark.
    pub fn publications(&self) -> impl Iterator<Item = &Publication> {
        self.faces.iter().filter_map(Face::publication)
    }

    /// Revoke all external queue grants and release subscriptions/deliveries.
    /// A failed object stays owned at the same index for another finish call.
    pub fn finish(&mut self) -> Result<(), CleanupError> {
        self.phase = Phase::Closing;
        if let Some(waiting) = self.waiting.as_mut() {
            waiting.finish().map_err(CleanupError::Mux)?;
            self.waiting = None;
        }
        for face in &mut self.faces {
            face.finish().map_err(CleanupError::Face)?;
        }
        self.phase = Phase::Closed;
        Ok(())
    }

    pub fn is_finished(&self) -> bool {
        matches!(self.phase, Phase::Closed)
    }
    pub fn task(&self) -> TaskId {
        self.task
    }
    pub fn step(&mut self, wait: Wait) -> Result<run::Report, Error> {
        run::step(self, wait)
    }
}

fn inspect(token: PendId) -> Result<PendInfo, Error> {
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())).map_err(Error::Inspect)?;
    let info = PendInfo::from_bytes(&bytes).ok_or(Error::InvalidConfig)?;
    if info.token != token || info.status != PendStatus::Active {
        return Err(Error::Dependency);
    }
    Ok(info)
}

impl Drop for Service {
    fn drop(&mut self) {
        if !self.is_finished() {
            let _ = self.finish();
        }
    }
}

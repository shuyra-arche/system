//! Same-Team native worker ownership, separate from execution completion.
use erra::Error as KernelError;
use kernel_api::{
    abi::call::unit::UnitFail,
    address::VirtAddr,
    model::{
        identity::TaskId,
        unit::{UnitCreate, UnitInfo, UnitQuery, UnitState, UnitTarget},
    },
};
use kernel_client::call::unit;

pub type Entry = unsafe extern "C" fn(*const usize, usize) -> !;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Prepared,
    Started,
    Stopping,
    Closed,
}
#[derive(Debug)]
pub enum Error {
    Arguments,
    Phase,
    Kernel(KernelError<UnitFail>),
    Observation,
}

/// Owns the creator's administrative Task record. It never treats Stop or Exit
/// as a reclaim proof, or moves Task-local Pend handles between worker tables.
#[derive(Debug)]
#[must_use = "stop, observe reclamation, and finish the worker record explicitly"]
pub struct Worker {
    task: TaskId,
    phase: Phase,
    observed: Option<UnitInfo>,
}
impl Worker {
    /// Prepare a dormant member of the caller's Team. Kernel copies the words
    /// before returning; it supplies a guarded stack and the RV64 a0/a1 entry.
    ///
    /// # Safety
    /// Any pointers embedded in `arguments` must obey `entry`'s lifetime and
    /// synchronization contract until the worker is observed reclaimed.
    /// Numeric Pend coordinates must refer to grants in the recipient's table,
    /// established before Resume, rather than the caller's local handles.
    pub unsafe fn prepare(
        entry: Entry,
        arguments: &[usize],
        stack_bytes: usize,
    ) -> Result<Self, Error> {
        if arguments.len() > 256 || stack_bytes > 16 * 1024 * 1024 {
            return Err(Error::Arguments);
        }
        let request = UnitCreate::Task {
            team: None,
            entry: VirtAddr::new(entry as *const () as usize),
            args: VirtAddr::new(arguments.as_ptr().addr()),
            count: arguments.len() as u64,
            stack: VirtAddr::new(stack_bytes),
        }
        .to_bytes();
        let target = unit::create(VirtAddr::new(request.as_ptr().addr())).map_err(Error::Kernel)?;
        let UnitTarget::Task(task) = target else {
            // A Task request returning a Team contradicts the trusted ABI.
            unreachable!("Kernel returned a non-Task for Task creation")
        };
        Ok(Self {
            task,
            phase: Phase::Prepared,
            observed: None,
        })
    }

    pub fn task(&self) -> TaskId {
        self.task
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn result(&self) -> Option<&UnitInfo> {
        self.observed.as_ref()
    }

    pub fn resume(&mut self) -> Result<(), Error> {
        if self.phase != Phase::Prepared {
            return Err(Error::Phase);
        }
        unit::resume(self.task.into()).map_err(Error::Kernel)?;
        self.phase = Phase::Started;
        Ok(())
    }

    /// Request cancellation. The record and any shared argument storage remain
    /// owned until independent observation proves the worker no longer uses it.
    pub fn stop(&mut self) -> Result<(), Error> {
        if self.phase == Phase::Closed {
            return Ok(());
        }
        unit::stop(self.task.into()).map_err(Error::Kernel)?;
        self.phase = Phase::Stopping;
        Ok(())
    }

    pub fn observe(&mut self) -> Result<&UnitInfo, Error> {
        if self.phase == Phase::Closed {
            return Err(Error::Phase);
        }
        let mut bytes = [0; UnitInfo::WIDTH];
        if !unit::inspect(
            UnitQuery::Target(self.task.into()),
            VirtAddr::new(bytes.as_mut_ptr().addr()),
        )
        .map_err(Error::Kernel)?
        {
            return Err(Error::Observation);
        }
        let info = UnitInfo::from_bytes(&bytes).ok_or(Error::Observation)?;
        if info.target != UnitTarget::Task(self.task)
            || info.reclaimed && info.state != UnitState::Exited
            || self.observed.is_some_and(|old| {
                old.reclaimed && !info.reclaimed
                    || old.exit.is_some() && (old.exit != info.exit || old.result != info.result)
            })
        {
            return Err(Error::Observation);
        }
        self.observed = Some(info);
        Ok(self.observed.as_ref().expect("stored observation"))
    }

    /// A false result retains the worker for the next supervisor round. On a
    /// failed Delete, including Busy, the completed record remains retryable.
    pub fn finish(&mut self) -> Result<bool, Error> {
        if self.phase == Phase::Closed {
            return Ok(true);
        }
        let info = self.observe()?;
        if info.state != UnitState::Exited || !info.reclaimed {
            return Ok(false);
        }
        unit::delete(self.task.into()).map_err(Error::Kernel)?;
        self.phase = Phase::Closed;
        Ok(true)
    }
}

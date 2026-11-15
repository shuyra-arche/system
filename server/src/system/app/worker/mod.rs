//! Same-Team native worker ownership, separate from execution completion.
use erra::Error as KernelError;
use kernel_api::{
    abi::call::unit::UnitFail,
    address::VirtAddr,
    model::{
        identity::TaskId,
        unit::{CreatedUnit, TaskStart, UnitCreate, UnitInfo, UnitQuery, UnitState, UnitTarget},
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
    stack: usize,
    phase: Phase,
    observed: Option<UnitInfo>,
}
impl Worker {
    /// Prepare Runtime-owned stack/arguments, then submit a dormant execution
    /// context. Backing remains held until separate Task and memory retirement.
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
        if arguments.len() > TaskStart::MAX_ARGUMENTS || stack_bytes > 16 * 1024 * 1024 {
            return Err(Error::Arguments);
        }
        let size = stack_bytes
            .max(65536)
            .checked_next_multiple_of(kernel_api::address::PAGE_SIZE)
            .ok_or(Error::Arguments)?;
        let stack = crate::runtime::heap::stack(size);
        if stack == 0 {
            return Err(Error::Arguments);
        }
        let at = stack + size - arguments.len() * 8;
        for (index, word) in arguments.iter().enumerate() {
            unsafe {
                ((at + index * 8) as *mut usize).write(*word);
            }
        }
        let request = UnitCreate::Task {
            team: None,
            start: TaskStart {
                entry: VirtAddr::new(entry as *const () as usize),
                stack_pointer: VirtAddr::new(at & !15),
                arguments: VirtAddr::new(at),
                argument_count: arguments.len(),
            },
        }
        .to_bytes();
        let target = match unit::create(VirtAddr::new(request.as_ptr().addr())) {
            Ok(value) => value,
            Err(error) => {
                crate::runtime::heap::release_stack(stack);
                return Err(Error::Kernel(error));
            }
        };
        let CreatedUnit::Task(task) = target else {
            // A Task request returning a Team contradicts the trusted ABI.
            unreachable!("Kernel returned a non-Task for Task creation")
        };
        Ok(Self {
            task,
            stack,
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
        crate::runtime::heap::release_stack(self.stack);
        self.phase = Phase::Closed;
        Ok(true)
    }
}

//! Retained ELF construction and separate Task/Team reclamation evidence.
use super::{
    Error, Image, Loader,
    image::elf,
    source::{Shared, Source},
};
use crate::runtime::view::Mapping;
use alloc::{rc::Rc, vec::Vec};
use kernel_api::{
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::Placement,
        identity::{Mark, PendId, TaskId, TeamId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, CreationRights, Distributable, Limits, NestRights,
            NestScope, Operations, PendScope,
        },
        unit::{ControlId, CreatedUnit, UnitCreate, UnitInfo, UnitQuery, UnitState, UnitTarget},
    },
};
use kernel_client::call::{pend, unit};

pub(crate) struct Spawn<'a> {
    pub args: &'a [usize],
    pub stack: usize,
    pub guard: ControlId,
    pub member_exit: bool,
    /// Trusted construction policy, never caller-provided image metadata.
    pub creation: CreationRights,
}
struct Segment {
    va: usize,
    size: usize,
    flags: u64,
    prefix: usize,
    offset: usize,
    len: usize,
    source: Option<Shared>,
    mapping: Option<Mapping>,
    cache_decided: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Preparing,
    Dormant,
    Running,
    Closing,
    Closed,
}
#[must_use = "retain partial construction; cancel/finish and observe Kernel reclamation"]
pub(crate) struct Job {
    image: Vec<u8>,
    nest: Option<PendId>,
    child_license: Option<PendId>,
    child_space: Option<PendId>,
    entry: usize,
    args: Vec<usize>,
    stack: usize,
    guard: ControlId,
    member_exit: bool,
    creation: CreationRights,
    segments: Vec<Segment>,
    cursor: usize,
    team: Option<TeamId>,
    task: Option<TaskId>,
    observed: Option<UnitInfo>,
    stopped: bool,
    phase: Phase,
}
impl Job {
    pub(super) fn new(image: Image<'_>, spawn: Spawn<'_>) -> Result<Self, Error> {
        if image.bytes.len() > system_protocol::loader::MAX_IMAGE
            || image.kind != super::ProgramKind::User
            || spawn.args.len() > system_protocol::loader::MAX_ARGS
            || spawn.stack > 1 << 20
        {
            return Err(Error::Invalid);
        }
        let plan =
            elf::parse::<PAGE_SIZE>(image.bytes, elf::Target::native().ok_or(Error::Invalid)?)
                .map_err(Error::Image)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(image.bytes.len())
            .map_err(|_| Error::Capacity)?;
        bytes.extend_from_slice(image.bytes);
        let mut args = Vec::new();
        args.try_reserve_exact(spawn.args.len())
            .map_err(|_| Error::Capacity)?;
        args.extend_from_slice(spawn.args);
        let mut segments = Vec::new();
        segments
            .try_reserve_exact(plan.regions.len().checked_mul(2).ok_or(Error::Invalid)?)
            .map_err(|_| Error::Capacity)?;
        for region in plan.regions {
            if region.data_size != 0 {
                segments.push(Segment {
                    va: region.va,
                    size: region.data_size,
                    flags: region.flags,
                    prefix: region.prefix,
                    offset: region.file_offset,
                    len: region.file_size,
                    source: None,
                    mapping: None,
                    cache_decided: false,
                });
            }
            if region.data_size < region.size {
                segments.push(Segment {
                    va: region
                        .va
                        .checked_add(region.data_size)
                        .ok_or(Error::Invalid)?,
                    size: region.size - region.data_size,
                    flags: region.flags,
                    prefix: 0,
                    offset: 0,
                    len: 0,
                    source: None,
                    mapping: None,
                    cache_decided: false,
                });
            }
        }
        Ok(Self {
            image: bytes,
            nest: None,
            child_license: None,
            child_space: None,
            entry: plan.entry,
            args,
            stack: spawn.stack,
            guard: spawn.guard,
            member_exit: spawn.member_exit,
            creation: spawn.creation,
            segments,
            cursor: 0,
            team: None,
            task: None,
            observed: None,
            stopped: false,
            phase: Phase::Preparing,
        })
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn team(&self) -> Option<TeamId> {
        self.team
    }
    pub fn task(&self) -> Option<TaskId> {
        self.task
    }
    pub fn result(&self) -> Option<&UnitInfo> {
        self.observed.as_ref()
    }
    pub fn advance(&mut self, loader: &mut Loader) -> Result<bool, Error> {
        match self.progress(loader) {
            Err(error) if error.busy() => Ok(false),
            other => other,
        }
    }
    fn progress(&mut self, loader: &mut Loader) -> Result<bool, Error> {
        if self.phase == Phase::Dormant {
            return Ok(true);
        }
        if self.phase != Phase::Preparing {
            return Err(Error::Invalid);
        }
        if self.team.is_none() {
            let request = UnitCreate::Team {
                guard: self.guard,
                nest: None,
                member_exit: self.member_exit,
            }
            .to_bytes();
            let CreatedUnit::Team { team, nest } =
                unit::create(addr(&request)).map_err(Error::Unit)?
            else {
                return Err(Error::Invalid);
            };
            self.team = Some(team);
            self.nest = Some(nest);
            return Ok(false);
        }
        if self.cursor < self.segments.len() {
            let segment = &mut self.segments[self.cursor];
            let contents = &self.image[segment.offset..segment.offset + segment.len];
            if segment.source.is_none() {
                if segment.flags & 4 == 0 {
                    segment.source =
                        loader
                            .cache
                            .find(contents, segment.prefix, segment.size, segment.flags);
                    segment.cache_decided = segment.source.is_some();
                }
                if segment.source.is_none() {
                    segment.source = Some(
                        Source::new(contents, segment.prefix, segment.size, segment.flags)?
                            .shared(),
                    );
                }
                return Ok(false);
            }
            let source = segment.source.as_ref().unwrap();
            if !source.borrow_mut().advance(loader.license, loader.space)? {
                return Ok(false);
            }
            if !segment.cache_decided {
                if segment.flags & 4 == 0 {
                    loader.cache.insert(source.clone());
                }
                segment.cache_decided = true;
            }
            if segment.mapping.is_none() {
                segment.mapping = Some(
                    Mapping::new(
                        self.nest.ok_or(Error::Invalid)?,
                        source.borrow().token().ok_or(Error::Invalid)?,
                        Placement::exact(VirtAddr::new(segment.va), PAGE_SIZE)
                            .ok_or(Error::Invalid)?,
                        segment.flags,
                        super::abort,
                    )
                    .map_err(Error::Mapping)?,
                );
                return Ok(false);
            }
            if segment
                .mapping
                .as_mut()
                .unwrap()
                .advance()
                .map_err(Error::Mapping)?
            {
                self.cursor += 1;
            }
            return Ok(false);
        }
        if self.task.is_none() {
            // ABI 11 prepares every segment through the creator's real Nest
            // authority. The new Task starts at the ELF entry with a fresh
            // native request journal; no injected initializer resets its IDs.
            let request = UnitCreate::Task {
                team: self.team,
                entry: VirtAddr::new(self.entry),
                args: VirtAddr::new(self.args.as_ptr().addr()),
                count: self.args.len() as u64,
                stack: VirtAddr::new(self.stack),
            }
            .to_bytes();
            let CreatedUnit::Task(task) = unit::create(addr(&request)).map_err(Error::Unit)? else {
                return Err(Error::Invalid);
            };
            self.task = Some(task);
            self.image = Vec::new();
            return Ok(false);
        }
        let task = self.task.unwrap();
        if !self.creation.is_empty() && self.child_license.is_none() {
            self.child_license = Some(grant(
                loader.license,
                task,
                Operations::None(self.creation),
                Modifiable::No,
            )?);
            return Ok(false);
        }
        if self.child_space.is_none() {
            self.child_space = Some(grant(
                self.nest.ok_or(Error::Invalid)?,
                task,
                Operations::Nest(NestRights::INSPECT | NestRights::RESERVE),
                Modifiable::Yes,
            )?);
            return Ok(false);
        }
        self.phase = Phase::Dormant;
        Ok(true)
    }
    pub fn resume(&mut self) -> Result<(), Error> {
        if self.phase != Phase::Dormant {
            return Err(Error::Invalid);
        }
        unit::resume(self.task.unwrap().into()).map_err(Error::Unit)?;
        self.phase = Phase::Running;
        Ok(())
    }
    pub fn observe(&mut self) -> Result<Option<&UnitInfo>, Error> {
        let Some(task) = self.task else {
            return Ok(self.observed.as_ref());
        };
        let info = inspect(task.into())?;
        if info.target != UnitTarget::Task(task) {
            return Err(Error::Invalid);
        }
        self.observed = Some(info);
        Ok(self.observed.as_ref())
    }
    pub fn cancel(&mut self) -> Result<(), Error> {
        if self.phase == Phase::Closed {
            return Ok(());
        }
        self.phase = Phase::Closing;
        if !self.stopped {
            if let Some(team) = self.team {
                unit::stop(team.into()).map_err(Error::Unit)?;
            }
            self.stopped = true;
        }
        Ok(())
    }
    /// Task exit/reclamation precedes mapping retirement. Whole-Team deletion
    /// must succeed before backing is released. Every partial stage is retained.
    pub fn finish(&mut self) -> Result<bool, Error> {
        if self.phase == Phase::Closed {
            return Ok(true);
        }
        if !self.stopped {
            if self.task.is_none() && self.team.is_none() {
                self.phase = Phase::Closed;
                return Ok(true);
            }
            if !self
                .observe()?
                .is_some_and(|info| info.state == UnitState::Exited)
            {
                return Ok(false);
            }
            self.cancel()?;
        }
        if let Some(task) = self.task {
            let info = inspect(task.into())?;
            if !info.reclaimed {
                return Ok(false);
            }
            self.observed = Some(info);
        }
        // A stopped/reclaimed Team cannot access these private reservations.
        // Release/Return them explicitly before asking Kernel to close its Nest.
        for segment in &mut self.segments {
            if let Some(mapping) = segment.mapping.as_mut() {
                if mapping.finish().map_err(Error::Mapping)? {
                    segment.mapping = None;
                }
                return Ok(false);
            }
        }
        if let Some(team) = self.team {
            match unit::delete(team.into()) {
                Ok(()) => {
                    self.team = None;
                    self.task = None;
                }
                Err(error) if error.source.is_busy() => return Ok(false),
                Err(error) => return Err(Error::Unit(error)),
            }
            return Ok(false);
        }
        for segment in &mut self.segments {
            if let Some(source) = segment.source.as_ref() {
                if Rc::strong_count(source) == 1 && !source.borrow_mut().finish()? {
                    return Ok(false);
                }
                segment.source = None;
                return Ok(false);
            }
        }
        if let Some(nest) = self.nest {
            pend::return_(nest).map_err(Error::Pend)?;
            self.nest = None;
            return Ok(false);
        }
        self.phase = Phase::Closed;
        Ok(true)
    }
}
fn addr(bytes: &[u8]) -> VirtAddr {
    VirtAddr::new(bytes.as_ptr().addr())
}
fn inspect(target: UnitTarget) -> Result<UnitInfo, Error> {
    let mut bytes = [0; UnitInfo::WIDTH];
    if !unit::inspect(
        UnitQuery::Target(target),
        VirtAddr::new(bytes.as_mut_ptr().addr()),
    )
    .map_err(Error::Unit)?
    {
        return Err(Error::Invalid);
    }
    UnitInfo::from_bytes(&bytes).ok_or(Error::Invalid)
}
fn grant(
    source: PendId,
    target: TaskId,
    operations: Operations,
    modifiable: Modifiable,
) -> Result<PendId, Error> {
    let request = AccordRequest {
        src: source,
        target: AccordTarget::Task(target),
        mark: Mark::NONE,
        limits: Limits {
            operations,
            modifiable,
            distributable: Distributable::No,
            scope: match operations {
                Operations::Nest(_) => PendScope::Nest(NestScope::Whole),
                _ => PendScope::Object,
            },
        },
    }
    .to_bytes();
    pend::accord(addr(&request)).map_err(Error::Pend)
}

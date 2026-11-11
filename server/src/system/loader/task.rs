//! A retained ELF construction and its separate Team/Task retirement records.
use super::bootstrap;
use super::{
    Error, Image, Loader,
    image::elf,
    memory::Mapping,
    source::{Shared, Source},
};
use alloc::{rc::Rc, vec::Vec};
use kernel_api::abi::call::sign::SignReply;
use kernel_api::model::{
    identity::{Mark, PendId},
    modifiable::Modifiable,
    pend::{
        AccordRequest, AccordTarget, CreationRights, Distributable, Limits, Operations, PendRange,
        ShareRights,
    },
    sign::SignRights,
    wait::Wait,
};
use kernel_api::{
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::{CellSource, Placement},
        identity::{TaskId, TeamId},
        unit::{ControlId, UnitCreate, UnitInfo, UnitQuery, UnitState, UnitTarget},
    },
};
use kernel_client::call::{pend, sign, unit};

pub(crate) struct Spawn<'a> {
    pub args: &'a [usize],
    pub stack: usize,
    pub guard: ControlId,
    pub member_exit: bool,
}
struct Segment {
    va: usize,
    size: usize,
    flags: u64,
    prefix: usize,
    embedded: Option<&'static [u8]>,
    delivered: Option<usize>,
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
    creator: TaskId,
    ready: Option<PendId>,
    child_ready: Option<usize>,
    child_license: Option<usize>,
    metadata: Option<Source>,
    metadata_granted: bool,
    initialized: bool,
    kind: kernel_api::model::unit::ProgramKind,
    entry: usize,
    args: Vec<usize>,
    stack: usize,
    guard: ControlId,
    member_exit: bool,
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
            || spawn.args.len() > system_protocol::loader::MAX_ARGS
            || spawn.stack > 1 << 20
        {
            return Err(Error::Invalid);
        }
        let target = elf::Target::native().ok_or(Error::Invalid)?;
        let plan = elf::parse::<PAGE_SIZE>(image.bytes, target).map_err(Error::Image)?;
        let mut caller = [0; UnitInfo::WIDTH];
        if !unit::inspect(UnitQuery::Caller, VirtAddr::new(caller.as_mut_ptr().addr()))
            .map_err(Error::Unit)?
        {
            return Err(Error::Invalid);
        }
        let UnitTarget::Task(creator) = UnitInfo::from_bytes(&caller).ok_or(Error::Invalid)?.target
        else {
            return Err(Error::Invalid);
        };
        let (bootstrap_bytes, bootstrap_va, bootstrap_size) = bootstrap::code()?;
        if plan.regions.iter().any(|region| {
            region.va < bootstrap_va + bootstrap_size && bootstrap_va < region.va + region.size
        }) {
            return Err(Error::Invalid);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(image.bytes.len())
            .map_err(|_| Error::Capacity)?;
        bytes.extend_from_slice(image.bytes);
        let mut args = Vec::new();
        args.try_reserve_exact(spawn.args.len() + 3)
            .map_err(|_| Error::Capacity)?;
        args.extend_from_slice(&[creator.get(), plan.entry, spawn.args.len()]);
        args.extend_from_slice(spawn.args);
        let mut segments = Vec::new();
        segments
            .try_reserve_exact(
                plan.regions
                    .len()
                    .checked_mul(2)
                    .and_then(|n| n.checked_add(1))
                    .ok_or(Error::Invalid)?,
            )
            .map_err(|_| Error::Capacity)?;
        for region in plan.regions {
            if region.data_size != 0 {
                segments.push(Segment {
                    va: region.va,
                    size: region.data_size,
                    flags: region.flags,
                    prefix: region.prefix,
                    embedded: None,
                    delivered: None,
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
                    embedded: None,
                    delivered: None,
                    offset: 0,
                    len: 0,
                    source: None,
                    mapping: None,
                    cache_decided: false,
                });
            }
        }
        segments.push(Segment {
            va: bootstrap_va,
            size: bootstrap_size,
            flags: 10,
            prefix: 0,
            embedded: Some(bootstrap_bytes),
            delivered: None,
            offset: 0,
            len: bootstrap_bytes.len(),
            source: None,
            mapping: None,
            cache_decided: false,
        });
        Ok(Self {
            image: bytes,
            creator,
            ready: None,
            child_ready: None,
            child_license: None,
            metadata: None,
            metadata_granted: false,
            initialized: false,
            kind: image.kind,
            entry: plan.entry,
            args,
            stack: spawn.stack,
            guard: spawn.guard,
            member_exit: spawn.member_exit,
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
        let result = self.progress(loader);
        match result {
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
                kind: self.kind,
                guard: self.guard,
                nest: None,
                member_exit: self.member_exit,
            }
            .to_bytes();
            let UnitTarget::Team(team) = unit::create(addr(&request)).map_err(Error::Unit)? else {
                return Err(Error::Invalid);
            };
            self.team = Some(team);
            return Ok(false);
        }
        if self.cursor < self.segments.len() {
            let segment = &mut self.segments[self.cursor];
            let contents = segment
                .embedded
                .unwrap_or_else(|| &self.image[segment.offset..segment.offset + segment.len]);
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
            if !source.borrow_mut().advance(loader.license)? {
                return Ok(false);
            }
            if !segment.cache_decided {
                if segment.flags & 4 == 0 {
                    loader.cache.insert(source.clone());
                }
                segment.cache_decided = true;
            }
            if segment.flags & 4 != 0 {
                self.cursor += 1;
                return Ok(false);
            }
            if segment.mapping.is_none() {
                let token = source.borrow().token().ok_or(Error::Invalid)?;
                segment.mapping = Some(
                    Mapping::new(
                        CellSource::Pend {
                            token,
                            offset: 0,
                            count: segment.size / PAGE_SIZE,
                        },
                        self.team.unwrap(),
                        Placement::exact(VirtAddr::new(segment.va), PAGE_SIZE)
                            .ok_or(Error::Invalid)?,
                        segment.flags,
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
            let request = UnitCreate::Task {
                team: self.team,
                entry: VirtAddr::new(bootstrap::enter as *const () as usize),
                args: VirtAddr::new(self.args.as_ptr().addr()),
                count: self.args.len() as u64,
                stack: VirtAddr::new(self.stack),
            }
            .to_bytes();
            let UnitTarget::Task(task) = unit::create(addr(&request)).map_err(Error::Unit)? else {
                return Err(Error::Invalid);
            };
            self.task = Some(task);
            self.image = Vec::new();
            return Ok(false);
        }
        let task = self.task.unwrap();
        if self.ready.is_none() {
            self.ready = Some(sign::create(loader.license).map_err(Error::Sign)?);
            return Ok(false);
        }
        if self.child_ready.is_none() {
            self.child_ready = Some(
                grant(
                    self.ready.unwrap(),
                    task,
                    Mark::NONE,
                    Operations::Sign(SignRights::TRIGGER),
                    Modifiable::No,
                )?
                .get(),
            );
            return Ok(false);
        }
        if self.child_license.is_none() {
            self.child_license = Some(
                grant(
                    loader.license,
                    task,
                    Mark::NONE,
                    Operations::None(CreationRights::all()),
                    Modifiable::No,
                )?
                .get(),
            );
            return Ok(false);
        }
        for segment in &mut self.segments {
            if segment.flags & 4 != 0 && segment.delivered.is_none() {
                segment.delivered = Some(
                    grant(
                        segment
                            .source
                            .as_ref()
                            .unwrap()
                            .borrow()
                            .token()
                            .ok_or(Error::Invalid)?,
                        task,
                        Mark::NONE,
                        Operations::Share(ShareRights::ASSOCIATE),
                        Modifiable::Yes,
                    )?
                    .get(),
                );
                return Ok(false);
            }
        }
        if self.metadata.is_none() {
            let mut records = Vec::new();
            records
                .try_reserve_exact(self.segments.len())
                .map_err(|_| Error::Capacity)?;
            for segment in &self.segments {
                if segment.flags & 4 != 0 {
                    records.push([
                        segment.delivered.unwrap(),
                        segment.va,
                        segment.size / PAGE_SIZE,
                        segment.flags as usize,
                    ]);
                }
            }
            let bytes = bootstrap::metadata(
                self.creator,
                self.entry,
                PendId::from_bytes(&(self.child_ready.unwrap() as u64).to_le_bytes()).unwrap(),
                &records,
            )?;
            let size = bytes
                .len()
                .checked_next_multiple_of(PAGE_SIZE)
                .ok_or(Error::Invalid)?;
            self.metadata = Some(Source::new(&bytes, 0, size, 2)?);
        }
        if !self.metadata.as_mut().unwrap().advance(loader.license)? {
            return Ok(false);
        }
        if !self.metadata_granted {
            grant(
                self.metadata
                    .as_ref()
                    .unwrap()
                    .token()
                    .ok_or(Error::Invalid)?,
                task,
                bootstrap::MARK,
                Operations::Share(ShareRights::OPEN | ShareRights::SHUT),
                Modifiable::No,
            )?;
            self.metadata_granted = true;
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
    /// Observe the authenticated initializer's quiescence before releasing its
    /// metadata. This is distinct from application-level readiness or exit.
    pub fn initialized(&mut self) -> Result<bool, Error> {
        if !self.initialized {
            let token = self.ready.ok_or(Error::Invalid)?;
            match sign::listen(token, Wait::Poll).map_err(Error::Sign)? {
                SignReply::Pending => return Ok(false),
                SignReply::Triggered { sequence } => {
                    sign::consume(token, sequence).map_err(Error::Sign)?;
                    self.initialized = true;
                }
                _ => return Err(Error::Invalid),
            }
        }
        if let Some(metadata) = self.metadata.as_mut() {
            metadata.finish()?;
            self.metadata = None;
        }
        if let Some(token) = self.ready {
            pend::return_(token).map_err(Error::Pend)?;
            self.ready = None;
        }
        Ok(true)
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
    /// Natural root exit closes the remaining Team before retirement. Kernel
    /// reclamation and successful Team deletion precede source cleanup; no Stop
    /// call is presented as proof that a Unit has relinquished memory.
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
        if let Some(team) = self.team {
            // Team's UnitInfo.reclaimed is not a completion channel in ABI 9.
            // Delete actively coordinates Nest retirement and returns Busy
            // until every member and mapping is safe. Success also clears this
            // controller's descendant Task receipts, so preserve ours above.
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
        if let Some(metadata) = self.metadata.as_mut() {
            metadata.finish()?;
            self.metadata = None;
            return Ok(false);
        }
        if let Some(token) = self.ready {
            pend::return_(token).map_err(Error::Pend)?;
            self.ready = None;
            return Ok(false);
        }
        for segment in &mut self.segments {
            if let Some(mapping) = segment.mapping.as_mut() {
                mapping.finish().map_err(Error::Mapping)?;
                segment.mapping = None;
                return Ok(false);
            }
            if let Some(source) = segment.source.as_ref() {
                if Rc::strong_count(source) == 1 {
                    source.borrow_mut().finish()?;
                }
                segment.source = None;
                return Ok(false);
            }
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
    mark: Mark,
    operations: Operations,
    modifiable: Modifiable,
) -> Result<PendId, Error> {
    let request = AccordRequest {
        src: source,
        target: AccordTarget::Task(target),
        mark,
        limits: Limits {
            operations,
            modifiable,
            distributable: Distributable::No,
            range: PendRange::Inherit,
        },
    }
    .to_bytes();
    pend::accord(addr(&request)).map_err(Error::Pend)
}

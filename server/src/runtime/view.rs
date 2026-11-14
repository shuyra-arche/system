//! A borrowed Flat/Share source and one independently owned CPU reservation.
use crate::runtime::memory::Memory;
use core::marker::PhantomData;
use kernel_api::{
    abi::{
        call::pend::PendFail,
        memory::{Action, Failure, Query, Snapshot, Source, Value},
    },
    address::{PAGE_SIZE, PageCount, VirtAddr},
    model::{
        cell::{CellLevel, Placement},
        flat::SlotPosition,
        identity::PendId,
        nest::{MemoPosition, NestId, ReserveRequest},
        pend::{
            FlatRights, Operations, PendInfo, PendRange, PendScope, PendStatus, ShareRights,
            Target, ViewTarget,
        },
    },
};
use kernel_client::call::pend;

#[derive(Debug)]
pub(crate) enum Error {
    Invalid,
    Pend(erra::Error<PendFail>),
    Native(Failure),
}
impl Error {
    pub fn busy(&self) -> bool {
        matches!(self, Self::Pend(error) if error.source.is_busy())
            || matches!(self, Self::Native(Failure::Busy | Failure::Stale))
    }
}
enum Phase {
    Reserve,
    Insert,
    Publish,
    Ready,
    Release,
    Return,
    Closed,
}

/// Keep the source alive until finish succeeds. This owner never returns the
/// borrowed source or Space grants. An unresolved native result is settled by
/// Memory with its original request identity before another transition occurs.
#[must_use = "finish the private reservation, retaining this owner on failure"]
pub(crate) struct Mapping {
    memory: Memory,
    source: PendId,
    target: Target,
    placement: Placement,
    access: u64,
    origin: usize,
    count: usize,
    space: PendId,
    nest: NestId,
    memo: Option<MemoPosition>,
    address: Option<VirtAddr>,
    page: usize,
    phase: Phase,
    task: PhantomData<*mut ()>,
}
impl Mapping {
    /// Foreign Nest addresses are observations; they cannot be dereferenced in
    /// this Task. The caller must retain the actual source throughout use.
    pub fn new(
        space: PendId,
        source: PendId,
        placement: Placement,
        access: u64,
        abort: fn(&'static str) -> !,
    ) -> Result<Self, Error> {
        let space_info = inspect(space)?;
        let Target::Nest(nest) = space_info.target else {
            return Err(Error::Invalid);
        };
        let info = inspect(source)?;
        let PendScope::Pages(PendRange::Pages { offset, count }) = info.limits.scope else {
            return Err(Error::Invalid);
        };
        if info.status != PendStatus::Active
            || space_info.status != PendStatus::Active
            || !match info.target {
                Target::Flat(_) => info
                    .limits
                    .operations
                    .contains(Operations::Flat(FlatRights::ASSOCIATE)),
                Target::View(ViewTarget::Share(_)) => info
                    .limits
                    .operations
                    .contains(Operations::Share(ShareRights::ASSOCIATE)),
                _ => false,
            }
            || access & 2 == 0
            || access & !14 != 0
            || access & 12 == 12
            || offset.checked_add(count).is_none()
            || count
                .checked_mul(PAGE_SIZE)
                .is_none_or(|n| n == 0 || n > isize::MAX as usize)
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            memory: Memory::new(abort),
            source,
            target: info.target,
            placement,
            access,
            origin: offset,
            count,
            space,
            nest,
            memo: None,
            address: None,
            page: 0,
            phase: Phase::Reserve,
            task: PhantomData,
        })
    }
    fn position(&self) -> Result<MemoPosition, Error> {
        let first = self.memo.ok_or(Error::Invalid)?;
        MemoPosition::new(first.token(), self.nest, first.reservation(), self.page)
            .ok_or(Error::Invalid)
    }
    pub fn advance(&mut self) -> Result<bool, Error> {
        match self.phase {
            Phase::Reserve => {
                let Snapshot::Nest(status) = self.memory.inspect(Query::Nest {
                    token: self.space,
                    nest: self.nest,
                }) else {
                    return Err(Error::Invalid);
                };
                let value = self
                    .memory
                    .mutate(Action::Reserve {
                        token: self.space,
                        nest: self.nest,
                        expected: status.version,
                        reservation: ReserveRequest {
                            placement: self.placement,
                            count: PageCount::new(self.count).ok_or(Error::Invalid)?,
                            level: CellLevel::BASE,
                            access: self.access,
                        },
                    })
                    .map_err(Error::Native)?;
                let Value::Reservation {
                    token,
                    reservation,
                    address,
                    ..
                } = value
                else {
                    return Err(Error::Invalid);
                };
                self.memo = Some(
                    MemoPosition::new(token, self.nest, reservation, 0).ok_or(Error::Invalid)?,
                );
                self.address = Some(address);
                self.phase = Phase::Insert;
            }
            Phase::Insert => {
                let target = self.position()?;
                let Snapshot::Memo(status) = self.memory.inspect(Query::Memo(target)) else {
                    return Err(Error::Invalid);
                };
                self.memory
                    .mutate(Action::Insert {
                        target,
                        expected: status.generation,
                        source: self.selected_source()?,
                    })
                    .map_err(Error::Native)?;
                self.phase = Phase::Publish;
            }
            Phase::Publish => {
                let memo = self.position()?;
                let Snapshot::Memo(status) = self.memory.inspect(Query::Memo(memo)) else {
                    return Err(Error::Invalid);
                };
                self.memory
                    .mutate(Action::Publish {
                        memo,
                        expected: status.generation,
                        maintenance: None,
                    })
                    .map_err(Error::Native)?;
                self.page += 1;
                self.phase = if self.page == self.count {
                    Phase::Ready
                } else {
                    Phase::Insert
                };
            }
            Phase::Ready => return Ok(true),
            Phase::Release | Phase::Return | Phase::Closed => return Err(Error::Invalid),
        }
        Ok(false)
    }
    fn selected_source(&self) -> Result<Source, Error> {
        let page = self.origin.checked_add(self.page).ok_or(Error::Invalid)?;
        match self.target {
            Target::View(ViewTarget::Share(view)) => Ok(Source::Share {
                token: self.source,
                view,
                page,
                count: 1,
            }),
            Target::Flat(flat) => {
                let position = SlotPosition::new(self.source, flat, page).ok_or(Error::Invalid)?;
                let Snapshot::Slot(status) = self.memory.inspect(Query::Slot(position)) else {
                    return Err(Error::Invalid);
                };
                Ok(Source::Flat {
                    position,
                    expected: status.version,
                })
            }
            _ => Err(Error::Invalid),
        }
    }
    pub fn is_ready(&self) -> bool {
        matches!(self.phase, Phase::Ready)
    }
    pub fn address(&self) -> Option<VirtAddr> {
        self.is_ready().then_some(self.address).flatten()
    }
    /// Descriptive virtual geometry only, including when the Nest is foreign.
    /// Dereferencing requires the caller's own live projection/source proof.
    pub fn span(&self) -> Option<(VirtAddr, usize)> {
        let base = self.address()?;
        let span = self.count.checked_mul(PAGE_SIZE)?;
        if base.get() == 0 || base.get().checked_add(span).is_none() {
            return None;
        }
        Some((base, span))
    }
    /// Release settles the entire private reservation, including partial opens.
    /// Returning its Pend is a separate successful transition. Busy retains both.
    pub fn finish(&mut self) -> Result<bool, Error> {
        let Some(memo) = self.memo else {
            self.phase = Phase::Closed;
            return Ok(true);
        };
        if !matches!(self.phase, Phase::Return | Phase::Closed) {
            self.phase = Phase::Release;
        }
        match self.phase {
            Phase::Release => {
                let Snapshot::Reservation(status) = self.memory.inspect(Query::Reservation {
                    token: memo.token(),
                    nest: memo.nest(),
                    reservation: memo.reservation(),
                }) else {
                    return Err(Error::Invalid);
                };
                self.memory
                    .mutate(Action::Release {
                        token: memo.token(),
                        nest: memo.nest(),
                        reservation: memo.reservation(),
                        expected: status.version,
                    })
                    .map_err(Error::Native)?;
                self.phase = Phase::Return;
                Ok(false)
            }
            Phase::Return => {
                pend::return_(memo.token()).map_err(Error::Pend)?;
                self.memo = None;
                self.address = None;
                self.phase = Phase::Closed;
                Ok(true)
            }
            Phase::Closed => Ok(true),
            _ => Err(Error::Invalid),
        }
    }
}
fn inspect(token: PendId) -> Result<PendInfo, Error> {
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())).map_err(Error::Pend)?;
    PendInfo::from_bytes(&bytes).ok_or(Error::Invalid)
}

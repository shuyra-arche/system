//! A borrowed Share source and one owned, read-only CPU reservation.
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
        identity::PendId,
        nest::{MemoPosition, NestId, ReserveRequest},
        pend::{
            Operations, PendInfo, PendRange, PendScope, PendStatus, ShareRights, Target, ViewTarget,
        },
        view::ViewId,
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
pub(crate) struct Read {
    memory: Memory,
    source: PendId,
    view: ViewId,
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
impl Read {
    /// # Safety
    /// Space must be this Task's actual Internal Nest. Use this object only in
    /// that Task; control of a foreign Nest does not map its VA into the caller.
    pub unsafe fn new(
        space: PendId,
        source: PendId,
        abort: fn(&'static str) -> !,
    ) -> Result<Self, Error> {
        let space_info = inspect(space)?;
        let Target::Nest(nest) = space_info.target else {
            return Err(Error::Invalid);
        };
        let info = inspect(source)?;
        let (
            Target::View(ViewTarget::Share(view)),
            PendScope::Pages(PendRange::Pages { offset, count }),
        ) = (info.target, info.limits.scope)
        else {
            return Err(Error::Invalid);
        };
        if info.status != PendStatus::Active
            || space_info.status != PendStatus::Active
            || !info
                .limits
                .operations
                .contains(Operations::Share(ShareRights::ASSOCIATE))
            || count
                .checked_mul(PAGE_SIZE)
                .is_none_or(|n| n == 0 || n > isize::MAX as usize)
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            memory: Memory::new(abort),
            source,
            view,
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
                            placement: Placement::lowest(PAGE_SIZE).ok_or(Error::Invalid)?,
                            count: PageCount::new(self.count).ok_or(Error::Invalid)?,
                            level: CellLevel::BASE,
                            access: 2,
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
                        source: Source::Share {
                            token: self.source,
                            view: self.view,
                            page: self.origin.checked_add(self.page).ok_or(Error::Invalid)?,
                            count: 1,
                        },
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
    /// # Safety
    /// The source is immutable and held against withdrawal throughout this
    /// borrow. The caller's current Nest is the one proven in new(). No Kernel
    /// call or other owner may replace/release this private reservation.
    pub unsafe fn bytes(&self) -> Result<&[u8], Error> {
        if !matches!(self.phase, Phase::Ready) {
            return Err(Error::Invalid);
        }
        let base = self.address.ok_or(Error::Invalid)?.get();
        let span = self.count.checked_mul(PAGE_SIZE).ok_or(Error::Invalid)?;
        if base == 0 || base.checked_add(span).is_none() {
            return Err(Error::Invalid);
        }
        Ok(unsafe { core::slice::from_raw_parts(base as *const u8, span) })
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

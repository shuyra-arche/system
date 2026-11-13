//! Native CPU Memo publication and fallible ownership cleanup.
use kernel_api::{
    abi::call::{cell::CellFail, nest::NestFail, pend::PendFail},
    address::{PageCount, VirtAddr},
    model::{
        cell::{
            CellAssociation, CellInfo, CellLevel, CellObject, CellPosition, CellState, Placement,
        },
        identity::PendId,
    },
};
use kernel_client::call::{cell, nest, pend};

#[derive(Debug)]
pub enum Error {
    Invalid,
    Cell(erra::Error<CellFail>),
    Pend(erra::Error<PendFail>),
    Nest(erra::Error<NestFail>),
}
impl Error {
    pub fn busy(&self) -> bool {
        match self {
            Self::Cell(error) => error.source.is_busy(),
            Self::Pend(error) => error.source.is_busy(),
            Self::Nest(error) => error.source.is_busy(),
            _ => false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Preparing,
    Publishing,
    Ready,
    Closing,
    Closed,
}

/// Owns one CPU Memo Pend. Source authority is borrowed until construction has
/// finished; the enclosing Source owner keeps the physical holding alive.
#[must_use = "finish the owned Memo and retain it after any cleanup failure"]
pub struct Mapping {
    source: CellPosition,
    nest: PendId,
    placement: Placement,
    access: u64,
    token: Option<PendId>,
    address: Option<VirtAddr>,
    count: usize,
    cursor: usize,
    phase: Phase,
}
impl Mapping {
    pub fn new(
        source: CellPosition,
        count: usize,
        nest: PendId,
        placement: Placement,
        access: u64,
    ) -> Result<Self, Error> {
        if count == 0
            || count
                .checked_mul(kernel_api::address::PAGE_SIZE)
                .is_none_or(|bytes| bytes > isize::MAX as usize)
            || access & 2 == 0
            || access & !14 != 0
            || access & 12 == 12
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            source,
            nest,
            placement,
            access,
            token: None,
            address: None,
            count,
            cursor: 0,
            phase: Phase::Preparing,
        })
    }
    pub fn address(&self) -> Option<VirtAddr> {
        if self.phase == Phase::Ready {
            self.address
        } else {
            None
        }
    }
    pub fn is_ready(&self) -> bool {
        self.phase == Phase::Ready
    }
    /// One state transition per round. Generation is observed immediately
    /// before Insert/Publish, and never guessed from a former ABI operation.
    pub fn advance(&mut self) -> Result<bool, Error> {
        if self.phase == Phase::Ready {
            return Ok(true);
        }
        if matches!(self.phase, Phase::Closing | Phase::Closed) {
            return Err(Error::Invalid);
        }
        let result = self.progress();
        match result {
            Err(error) if error.busy() => Ok(false),
            other => other,
        }
    }
    fn progress(&mut self) -> Result<bool, Error> {
        if self.token.is_none() {
            let (token, address) = nest::reserve(
                self.nest,
                self.placement,
                PageCount::new(self.count).ok_or(Error::Invalid)?,
                CellLevel::BASE,
                self.access,
            )
            .map_err(Error::Nest)?;
            self.token = Some(token);
            self.address = Some(address);
            self.phase = Phase::Publishing;
            return Ok(false);
        }
        let mut position = CellPosition {
            object: CellObject::Pend(self.token.unwrap()),
            page: self.cursor,
            generation: 0,
        };
        let mut bytes = [0; CellInfo::WIDTH];
        cell::inspect(
            addr(&position.to_bytes()),
            VirtAddr::new(bytes.as_mut_ptr().addr()),
        )
        .map_err(Error::Cell)?;
        let info = CellInfo::from_bytes(&bytes).ok_or(Error::Invalid)?;
        if info.external || info.level.get() != 0 {
            return Err(Error::Invalid);
        }
        position.generation = info.generation;
        match info.state {
            CellState::Vacant => {
                let source = CellPosition {
                    page: self
                        .source
                        .page
                        .checked_add(self.cursor)
                        .ok_or(Error::Invalid)?,
                    ..self.source
                };
                cell::insert(addr(
                    &CellAssociation {
                        source,
                        target: position,
                    }
                    .to_bytes(),
                ))
                .map_err(Error::Cell)?;
            }
            CellState::Ready => cell::publish(addr(&position.to_bytes())).map_err(Error::Cell)?,
            CellState::Published => {
                self.cursor += 1;
                if self.cursor == self.count {
                    self.phase = Phase::Ready;
                    return Ok(true);
                }
            }
            CellState::Publishing | CellState::Retracting => (),
            CellState::Failed => return Err(Error::Invalid),
        }
        Ok(false)
    }
    /// Returning the owned root settles its whole CPU relationship, including
    /// retained invalidation. Busy leaves the same Pend and record available.
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.phase == Phase::Closed {
            return Ok(());
        }
        self.phase = Phase::Closing;
        if let Some(token) = self.token {
            pend::return_(token).map_err(Error::Pend)?;
            self.token = None;
        }
        self.address = None;
        self.phase = Phase::Closed;
        Ok(())
    }
}
fn addr(bytes: &[u8]) -> VirtAddr {
    VirtAddr::new(bytes.as_ptr().addr())
}

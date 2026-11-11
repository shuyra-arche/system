//! A loader-owned image source, retained by every live target projection.
use super::{Error, memory::Mapping};
use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;
use kernel_api::{
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::{
            Allocation, CellCreate, CellObject, CellSource, CreatedCell, Placement, ReferenceRange,
            SlotId,
        },
        identity::{PendId, TeamId},
        modifiable::Modifiable,
    },
};
use kernel_client::call::{cell, pend, share_view};

pub(super) type Shared = Rc<RefCell<Source>>;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Allocate,
    Map,
    Copy,
    Share,
    Unmap,
    Release,
    Ready,
    Closing,
    Closed,
}
pub(super) struct Source {
    contents: Vec<u8>,
    prefix: usize,
    size: usize,
    flags: u64,
    phase: Phase,
    slot: Option<SlotId>,
    mapping: Option<Mapping>,
    share: Option<PendId>,
}
impl Source {
    pub fn new(contents: &[u8], prefix: usize, size: usize, flags: u64) -> Result<Self, Error> {
        if size == 0
            || !size.is_multiple_of(PAGE_SIZE)
            || prefix
                .checked_add(contents.len())
                .is_none_or(|end| end > size)
        {
            return Err(Error::Invalid);
        }
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(contents.len())
            .map_err(|_| Error::Capacity)?;
        owned.extend_from_slice(contents);
        Ok(Self {
            contents: owned,
            prefix,
            size,
            flags,
            phase: Phase::Allocate,
            slot: None,
            mapping: None,
            share: None,
        })
    }
    pub fn shared(self) -> Shared {
        Rc::new(RefCell::new(self))
    }
    pub fn pages(&self) -> usize {
        self.size / PAGE_SIZE
    }
    pub fn matches(&self, contents: &[u8], prefix: usize, size: usize, flags: u64) -> bool {
        self.phase == Phase::Ready
            && self.flags & 4 == 0
            && self.prefix == prefix
            && self.size == size
            && self.flags == flags
            && self.contents == contents
    }
    pub fn token(&self) -> Option<PendId> {
        if self.phase == Phase::Ready {
            self.share
        } else {
            None
        }
    }
    pub fn advance(&mut self, license: PendId) -> Result<bool, Error> {
        let result = self.progress(license);
        match result {
            Err(error) if error.busy() => Ok(false),
            other => other,
        }
    }
    fn progress(&mut self, license: PendId) -> Result<bool, Error> {
        match self.phase {
            Phase::Allocate => {
                let spec =
                    CellCreate::Slot(Allocation::new(self.size, PAGE_SIZE).ok_or(Error::Invalid)?)
                        .to_bytes();
                let CreatedCell::Slot(slot) =
                    cell::create(license, addr(&spec)).map_err(Error::Cell)?
                else {
                    return Err(Error::Invalid);
                };
                self.slot = Some(slot);
                self.phase = Phase::Map;
            }
            Phase::Map => {
                if self.mapping.is_none() {
                    self.mapping = Some(
                        Mapping::new(
                            CellSource::Slot(ReferenceRange {
                                reference: self.slot.unwrap(),
                                offset: 0,
                                count: self.pages(),
                            }),
                            TeamId::new(0),
                            Placement::lowest(PAGE_SIZE).unwrap(),
                            6,
                        )
                        .map_err(Error::Mapping)?,
                    );
                }
                if self
                    .mapping
                    .as_mut()
                    .unwrap()
                    .advance()
                    .map_err(Error::Mapping)?
                {
                    self.phase = Phase::Copy;
                }
            }
            Phase::Copy => {
                let address = self
                    .mapping
                    .as_ref()
                    .unwrap()
                    .address()
                    .ok_or(Error::Invalid)?
                    .get();
                // SAFETY: this Task owns the entire initialized private writable
                // mapping. No Share or other Task can yet withdraw/modify it.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        self.contents.as_ptr(),
                        (address + self.prefix) as *mut u8,
                        self.contents.len(),
                    );
                }
                if self.flags & 4 != 0 {
                    self.contents = Vec::new();
                }
                self.phase = Phase::Share;
            }
            Phase::Share => {
                self.share = Some(
                    share_view::create(
                        license,
                        self.slot.unwrap(),
                        if self.flags & 4 != 0 {
                            Modifiable::Yes
                        } else {
                            Modifiable::No
                        },
                    )
                    .map_err(Error::Share)?,
                );
                self.phase = Phase::Unmap;
            }
            Phase::Unmap => {
                if let Some(mapping) = self.mapping.as_mut() {
                    mapping.finish().map_err(Error::Mapping)?;
                    self.mapping = None;
                }
                self.phase = Phase::Release;
            }
            Phase::Release => {
                cell::delete(addr(&CellObject::Slot(self.slot.unwrap()).to_bytes()))
                    .map_err(Error::Cell)?;
                self.slot = None;
                self.phase = Phase::Ready;
            }
            Phase::Ready => return Ok(true),
            Phase::Closing | Phase::Closed => return Err(Error::Invalid),
        }
        Ok(false)
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        if self.phase == Phase::Closed {
            return Ok(());
        }
        self.phase = Phase::Closing;
        if let Some(mapping) = self.mapping.as_mut() {
            mapping.finish().map_err(Error::Mapping)?;
            self.mapping = None;
        }
        if let Some(token) = self.share {
            pend::return_(token).map_err(Error::Pend)?;
            self.share = None;
        }
        if let Some(slot) = self.slot {
            cell::delete(addr(&CellObject::Slot(slot).to_bytes())).map_err(Error::Cell)?;
            self.slot = None;
        }
        self.phase = Phase::Closed;
        Ok(())
    }
}
fn addr(bytes: &[u8]) -> VirtAddr {
    VirtAddr::new(bytes.as_ptr().addr())
}

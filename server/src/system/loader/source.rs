//! A loader-owned image source, retained by every live target projection.
use super::Error;
use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;
use kernel_api::{
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::{Allocation, SlotId},
        identity::PendId,
        modifiable::Modifiable,
        pend::{NarrowRequest, PendInfo},
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
    address: Option<VirtAddr>,
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
            address: None,
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
                let slot = cell::create(
                    license,
                    Allocation::new(self.size, PAGE_SIZE).ok_or(Error::Invalid)?,
                )
                .map_err(Error::Cell)?;
                self.slot = Some(slot);
                self.phase = Phase::Map;
            }
            Phase::Map => {
                if self.share.is_none() {
                    self.share = Some(
                        share_view::create(license, self.slot.unwrap(), Modifiable::Yes)
                            .map_err(Error::Share)?,
                    );
                    return Ok(false);
                }
                let (address, size) =
                    share_view::open(self.share.unwrap()).map_err(Error::Share)?;
                if size != self.size {
                    return Err(Error::Invalid);
                }
                self.address = Some(address);
                self.phase = Phase::Copy;
            }
            Phase::Copy => {
                let address = self.address.ok_or(Error::Invalid)?.get();
                // SAFETY: this Task owns the unpublished Share root and its
                // writable mapping. No descendant has been granted yet.
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
                self.phase = Phase::Unmap;
            }
            Phase::Share => {
                let token = self.share.unwrap();
                let mut bytes = [0; PendInfo::SIZE];
                pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr()))
                    .map_err(Error::Pend)?;
                let mut limits = PendInfo::from_bytes(&bytes).ok_or(Error::Invalid)?.limits;
                limits.modifiable = if self.flags & 4 != 0 {
                    Modifiable::Yes
                } else {
                    Modifiable::No
                };
                pend::narrow(addr(&NarrowRequest { token, limits }.to_bytes()))
                    .map_err(Error::Pend)?;
                self.phase = Phase::Release;
            }
            Phase::Unmap => {
                share_view::shut(self.share.unwrap()).map_err(Error::Share)?;
                self.address = None;
                self.phase = Phase::Share;
            }
            Phase::Release => {
                cell::delete(self.slot.unwrap()).map_err(Error::Cell)?;
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
        if self.address.is_some() {
            share_view::shut(self.share.ok_or(Error::Invalid)?).map_err(Error::Share)?;
            self.address = None;
        }
        if let Some(token) = self.share {
            pend::return_(token).map_err(Error::Pend)?;
            self.share = None;
        }
        if let Some(slot) = self.slot {
            cell::delete(slot).map_err(Error::Cell)?;
            self.slot = None;
        }
        self.phase = Phase::Closed;
        Ok(())
    }
}
fn addr(bytes: &[u8]) -> VirtAddr {
    VirtAddr::new(bytes.as_ptr().addr())
}

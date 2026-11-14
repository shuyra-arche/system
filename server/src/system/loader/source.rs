//! Loader-owned native Flat backing, retained by every live image projection.
use super::Error;
use crate::runtime::{memory::Memory, view::Mapping};
use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;
use kernel_api::{
    abi::memory::{Action, Query, Snapshot, Value},
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::{CellLevel, Placement},
        flat::{FlatId, SlotPosition},
        identity::{Mark, PendId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, Distributable, Limits, Operations, PendRange, PendScope,
            ShareRights,
        },
    },
};
use kernel_client::call::{pend, share_view};

pub(super) type Shared = Rc<RefCell<Source>>;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Allocate,
    Occupy,
    Map,
    Copy,
    Unmap,
    Share,
    Ready,
    Closing,
    Closed,
}
pub(super) struct Source {
    memory: Memory,
    contents: Vec<u8>,
    prefix: usize,
    size: usize,
    flags: u64,
    phase: Phase,
    flat: Option<(PendId, FlatId)>,
    cursor: usize,
    mapping: Option<Mapping>,
    share: Option<PendId>,
    deleted: bool,
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
            memory: Memory::new(super::abort),
            contents: owned,
            prefix,
            size,
            flags,
            phase: Phase::Allocate,
            flat: None,
            cursor: 0,
            mapping: None,
            share: None,
            deleted: false,
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
        (self.phase == Phase::Ready).then_some(self.share).flatten()
    }
    pub fn advance(&mut self, license: PendId, space: PendId) -> Result<bool, Error> {
        match self.progress(license, space) {
            Err(error) if error.busy() => Ok(false),
            other => other,
        }
    }
    fn progress(&mut self, license: PendId, space: PendId) -> Result<bool, Error> {
        match self.phase {
            Phase::Allocate => {
                let Value::CreatedFlat { token, flat, .. } = self
                    .memory
                    .mutate(Action::CreateFlat {
                        license,
                        count: self.pages(),
                        level: CellLevel::BASE,
                    })
                    .map_err(Error::Native)?
                else {
                    return Err(Error::Invalid);
                };
                self.flat = Some((token, flat));
                self.phase = Phase::Occupy;
            }
            Phase::Occupy => {
                if self.cursor == self.pages() {
                    self.phase = Phase::Map;
                    return Ok(false);
                }
                let (token, flat) = self.flat.ok_or(Error::Invalid)?;
                let slot = SlotPosition::new(token, flat, self.cursor).ok_or(Error::Invalid)?;
                let Snapshot::Slot(status) = self.memory.inspect(Query::Slot(slot)) else {
                    return Err(Error::Invalid);
                };
                self.memory
                    .mutate(Action::Occupy {
                        slot,
                        expected: status.version,
                    })
                    .map_err(Error::Native)?;
                self.cursor += 1;
            }
            Phase::Map => {
                if self.mapping.is_none() {
                    self.mapping = Some(
                        Mapping::new(
                            space,
                            self.flat.ok_or(Error::Invalid)?.0,
                            Placement::lowest(PAGE_SIZE).ok_or(Error::Invalid)?,
                            6,
                            super::abort,
                        )
                        .map_err(Error::Mapping)?,
                    );
                    return Ok(false);
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
                let (address, span) = self
                    .mapping
                    .as_ref()
                    .and_then(Mapping::span)
                    .ok_or(Error::Invalid)?;
                if span != self.size {
                    return Err(Error::Invalid);
                }
                let address = address.get();
                // SAFETY: Loader's trusted current Space holds this exclusive
                // writable projection of fresh zeroed backing. No Share exists.
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
            Phase::Unmap => {
                if self
                    .mapping
                    .as_mut()
                    .ok_or(Error::Invalid)?
                    .finish()
                    .map_err(Error::Mapping)?
                {
                    self.mapping = None;
                    self.phase = Phase::Share;
                }
            }
            Phase::Share => {
                let request = AccordRequest {
                    src: self.flat.ok_or(Error::Invalid)?.0,
                    target: AccordTarget::Caller,
                    mark: Mark::NONE,
                    limits: Limits {
                        operations: Operations::Share(ShareRights::ASSOCIATE),
                        modifiable: if self.flags & 4 != 0 {
                            Modifiable::Yes
                        } else {
                            Modifiable::No
                        },
                        distributable: Distributable::Yes,
                        scope: PendScope::Pages(PendRange::Inherit),
                    },
                }
                .to_bytes();
                self.share =
                    Some(share_view::derive(license, addr(&request)).map_err(Error::Share)?);
                self.phase = Phase::Ready;
            }
            Phase::Ready => return Ok(true),
            Phase::Closing | Phase::Closed => return Err(Error::Invalid),
        }
        Ok(false)
    }
    /// One retained cleanup transition per call. Source grants and Flat backing
    /// remain owned until all projections/descendants have actually retired.
    pub fn finish(&mut self) -> Result<bool, Error> {
        if self.phase == Phase::Closed {
            return Ok(true);
        }
        self.phase = Phase::Closing;
        if let Some(mapping) = self.mapping.as_mut() {
            if mapping.finish().map_err(Error::Mapping)? {
                self.mapping = None;
            }
            return Ok(false);
        }
        if let Some(token) = self.share {
            pend::return_(token).map_err(Error::Pend)?;
            self.share = None;
            return Ok(false);
        }
        if let Some((token, flat)) = self.flat {
            if !self.deleted {
                let Snapshot::Flat(status) = self.memory.inspect(Query::Flat { token, flat })
                else {
                    return Err(Error::Invalid);
                };
                self.memory
                    .mutate(Action::DeleteFlat {
                        token,
                        flat,
                        expected: status.version,
                    })
                    .map_err(Error::Native)?;
                self.deleted = true;
                return Ok(false);
            }
            pend::return_(token).map_err(Error::Pend)?;
            self.flat = None;
        }
        self.phase = Phase::Closed;
        Ok(true)
    }
}
fn addr(bytes: &[u8]) -> VirtAddr {
    VirtAddr::new(bytes.as_ptr().addr())
}

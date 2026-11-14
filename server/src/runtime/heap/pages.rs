//! The broker retains Flat holding and Reservation authority until every
//! publication is retracted, every association removed and release has settled.
use super::room;
use crate::runtime::memory::Memory;
use kernel_api::{
    abi::memory::{Action, Failure, Query, Snapshot, Source, Value},
    address::{PAGE_SIZE, PageCount},
    model::{
        cell::{CellLevel, Placement},
        flat::SlotPosition,
        identity::PendId,
        nest::{MemoPhase, MemoPosition, NestId},
        pend::{PendInfo, Target},
    },
};
use kernel_client::call::pend;

pub(super) struct Pages {
    memory: Memory,
    license: PendId,
    space: PendId,
    nest: NestId,
}
struct Header {
    flat: SlotPosition,
    memo: MemoPosition,
    count: usize,
}

impl Pages {
    pub(super) fn new(license: PendId, space: PendId) -> Self {
        let mut bytes = [0; PendInfo::SIZE];
        let deadline = room::clock().saturating_add(5_000_000_000);
        loop {
            match pend::inspect(
                space,
                kernel_api::address::VirtAddr::new(bytes.as_mut_ptr() as usize),
            ) {
                Ok(()) => break,
                Err(error) if error.source.is_busy() && room::clock() < deadline => room::yield_(),
                Err(_) => super::fatal(),
            }
        }
        let Some(PendInfo {
            target: Target::Nest(nest),
            ..
        }) = PendInfo::from_bytes(&bytes)
        else {
            super::fatal();
        };
        Self {
            memory: Memory::new(|_| super::fatal()),
            license,
            space,
            nest,
        }
    }

    // Only rejected operations may get a new identity. Busy/Stale snapshots
    // are refreshed; accepted operations settle inside Memory::mutate.
    fn change(&mut self, action: impl Fn(&Memory) -> Action) -> Result<Value, Failure> {
        let deadline = room::clock().saturating_add(5_000_000_000);
        loop {
            match self.memory.mutate(action(&self.memory)) {
                Err(Failure::Busy | Failure::Stale) if room::clock() < deadline => room::yield_(),
                result => return result,
            }
        }
    }

    fn allocate(&mut self, size: usize, align: usize) -> usize {
        let align = align.max(align_of::<Header>());
        let Some(prefix) = size_of::<Header>().checked_next_multiple_of(align) else {
            return 0;
        };
        let Some(bytes) = prefix
            .checked_add(size)
            .and_then(|n| n.checked_next_multiple_of(PAGE_SIZE))
            .filter(|&n| n <= isize::MAX as usize)
        else {
            return 0;
        };
        let count = bytes / PAGE_SIZE;
        let Some(placement) = Placement::lowest(align.max(PAGE_SIZE)) else {
            return 0;
        };
        let license = self.license;
        let Ok(Value::CreatedFlat { token, flat, .. }) = self.change(|_| Action::CreateFlat {
            license,
            count,
            level: CellLevel::new(0).unwrap(),
        }) else {
            return 0;
        };
        let flat = SlotPosition::new(token, flat, 0).unwrap();
        let space = self.space;
        let nest = self.nest;
        let result = self.change(|memory| {
            let Snapshot::Nest(status) = memory.inspect(Query::Nest { token: space, nest }) else {
                super::fatal();
            };
            Action::Reserve {
                token: space,
                nest,
                expected: status.version,
                reservation: kernel_api::model::nest::ReserveRequest {
                    placement,
                    count: PageCount::new(count).unwrap(),
                    level: CellLevel::new(0).unwrap(),
                    access: 6,
                },
            }
        });
        let Ok(Value::Reservation {
            token,
            reservation,
            address: base,
            ..
        }) = result
        else {
            self.delete(flat);
            return 0;
        };
        let memo = MemoPosition::new(token, nest, reservation, 0).unwrap();
        for page in 0..count {
            let slot = SlotPosition::new(flat.token(), flat.flat(), page).unwrap();
            let target = MemoPosition::new(token, nest, reservation, page).unwrap();
            let occupied = self.change(|memory| {
                let Snapshot::Slot(status) = memory.inspect(Query::Slot(slot)) else {
                    super::fatal()
                };
                Action::Occupy {
                    slot,
                    expected: status.version,
                }
            });
            let inserted = occupied.and_then(|_| {
                self.change(|memory| {
                    let Snapshot::Slot(source) = memory.inspect(Query::Slot(slot)) else {
                        super::fatal()
                    };
                    let Snapshot::Memo(status) = memory.inspect(Query::Memo(target)) else {
                        super::fatal()
                    };
                    Action::Insert {
                        target,
                        expected: status.generation,
                        source: Source::Flat {
                            position: slot,
                            expected: source.version,
                        },
                    }
                })
            });
            let published = inserted.and_then(|_| {
                self.change(|memory| {
                    let Snapshot::Memo(status) = memory.inspect(Query::Memo(target)) else {
                        super::fatal()
                    };
                    Action::Publish {
                        memo: target,
                        expected: status.generation,
                        maintenance: None,
                    }
                })
            });
            if published.is_err() {
                self.retire(memo, count);
                self.delete(flat);
                return 0;
            }
        }
        let data = base.get() + prefix;
        unsafe {
            ((data - size_of::<Header>()) as *mut Header).write(Header { flat, memo, count });
        }
        data
    }

    fn delete(&mut self, flat: SlotPosition) {
        self.change(|memory| {
            let Snapshot::Flat(status) = memory.inspect(Query::Flat {
                token: flat.token(),
                flat: flat.flat(),
            }) else {
                super::fatal()
            };
            Action::DeleteFlat {
                token: flat.token(),
                flat: flat.flat(),
                expected: status.version,
            }
        })
        .unwrap_or_else(|_| super::fatal());
        self.withdraw(flat.token());
    }

    fn withdraw(&self, token: PendId) {
        let deadline = room::clock().saturating_add(5_000_000_000);
        loop {
            match pend::return_(token) {
                Ok(()) => return,
                Err(error) if error.source.is_busy() && room::clock() < deadline => room::yield_(),
                Err(_) => super::fatal(),
            }
        }
    }

    fn retire(&mut self, first: MemoPosition, count: usize) {
        for page in 0..count {
            let memo =
                MemoPosition::new(first.token(), first.nest(), first.reservation(), page).unwrap();
            let Snapshot::Memo(status) = self.memory.inspect(Query::Memo(memo)) else {
                super::fatal()
            };
            if status.phase == MemoPhase::Published {
                // Poll the exact accepted Retract, never reconstruct it from
                // an intermediate generation or infer retirement from a wake.
                self.change(|memory| {
                    let Snapshot::Memo(status) = memory.inspect(Query::Memo(memo)) else {
                        super::fatal()
                    };
                    Action::Retract {
                        memo,
                        expected: status.generation,
                    }
                })
                .unwrap_or_else(|_| super::fatal());
            }
            let Snapshot::Memo(status) = self.memory.inspect(Query::Memo(memo)) else {
                super::fatal()
            };
            if status.phase == MemoPhase::Associated {
                self.change(|memory| {
                    let Snapshot::Memo(status) = memory.inspect(Query::Memo(memo)) else {
                        super::fatal()
                    };
                    Action::Remove {
                        memo,
                        expected: status.generation,
                    }
                })
                .unwrap_or_else(|_| super::fatal());
            } else if status.phase != MemoPhase::Empty {
                super::fatal();
            }
        }
        self.change(|memory| {
            let Snapshot::Reservation(status) = memory.inspect(Query::Reservation {
                token: first.token(),
                nest: first.nest(),
                reservation: first.reservation(),
            }) else {
                super::fatal()
            };
            Action::Release {
                token: first.token(),
                nest: first.nest(),
                reservation: first.reservation(),
                expected: status.version,
            }
        })
        .unwrap_or_else(|_| super::fatal());
        self.withdraw(first.token());
    }

    fn free(&mut self, data: usize) {
        // Copy every retained authority before withdrawing its containing mapping.
        let Header { flat, memo, count } =
            unsafe { ((data - size_of::<Header>()) as *const Header).read() };
        self.retire(memo, count);
        self.delete(flat);
    }
}

pub(super) fn allocate(license: PendId, size: usize, align: usize) -> usize {
    let data = Pages::new(license, super::space()).allocate(size, align);
    if data != 0 {
        super::MAPPINGS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }
    data
}
pub(super) fn free(data: usize) {
    // The root remains the only resolver of these Task-local authorities.
    Pages::new(PendId::NONE, super::space()).free(data);
    super::MAPPINGS.fetch_sub(1, core::sync::atomic::Ordering::Relaxed);
}

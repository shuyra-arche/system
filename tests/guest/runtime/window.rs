//! Relative windows are resolved once before recipient delivery, using ABI 11.
use crate::{
    addr, inspect,
    memory::Memory,
    native_supply::{Resource, Source as Leases},
};
use alloc::string::String;
use kernel_api::{
    abi::memory::{Action, Failure, Query, Snapshot, Source, Value},
    address::{PAGE_SIZE, PageCount, VirtAddr},
    model::{
        cell::{CellLevel, Placement},
        flat::SlotPosition,
        identity::{Mark, PendId},
        modifiable::Modifiable,
        nest::{MemoPosition, NestId, ReserveRequest},
        pend::{
            AccordRequest, AccordTarget, Distributable, Limits, Operations, PendInfo, PendRange,
            PendScope, ShareRights, Target,
        },
        unit::{UnitQuery, UnitTarget},
    },
};
use kernel_client::call::{pend, room, share_view};
use system_protocol::{supply as api, value::Rights};

fn change(memory: &mut Memory, action: impl Fn(&Memory) -> Action) -> Value {
    let deadline = room::clock() + 5_000_000_000;
    loop {
        match memory.mutate(action(memory)) {
            Ok(value) => return value,
            Err(Failure::Busy | Failure::Stale) if room::clock() < deadline => room::yield_(),
            error => panic!("native mutation: {error:?}"),
        }
    }
}
fn info(token: PendId) -> PendInfo {
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap();
    PendInfo::from_bytes(&bytes).unwrap()
}
fn reserve(
    memory: &mut Memory,
    space: PendId,
    nest: NestId,
    count: usize,
    access: u64,
) -> (MemoPosition, VirtAddr) {
    let Value::Reservation {
        token,
        reservation,
        address,
        ..
    } = change(memory, |memory| {
        let Snapshot::Nest(status) = memory.inspect(Query::Nest { token: space, nest }) else {
            panic!("Nest")
        };
        Action::Reserve {
            token: space,
            nest,
            expected: status.version,
            reservation: ReserveRequest {
                placement: Placement::lowest(PAGE_SIZE).unwrap(),
                count: PageCount::new(count).unwrap(),
                level: CellLevel::BASE,
                access,
            },
        }
    })
    else {
        panic!("Reservation")
    };
    (
        MemoPosition::new(token, nest, reservation, 0).unwrap(),
        address,
    )
}
fn publish(memory: &mut Memory, target: MemoPosition, source: Source) {
    change(memory, |memory| {
        let Snapshot::Memo(status) = memory.inspect(Query::Memo(target)) else {
            panic!("Memo")
        };
        Action::Insert {
            target,
            expected: status.generation,
            source,
        }
    });
    change(memory, |memory| {
        let Snapshot::Memo(status) = memory.inspect(Query::Memo(target)) else {
            panic!("Memo")
        };
        Action::Publish {
            memo: target,
            expected: status.generation,
            maintenance: None,
        }
    });
}
fn release(memory: &mut Memory, memo: MemoPosition) {
    change(memory, |memory| {
        let Snapshot::Reservation(status) = memory.inspect(Query::Reservation {
            token: memo.token(),
            nest: memo.nest(),
            reservation: memo.reservation(),
        }) else {
            panic!("Reservation")
        };
        Action::Release {
            token: memo.token(),
            nest: memo.nest(),
            reservation: memo.reservation(),
            expected: status.version,
        }
    });
    pend::return_(memo.token()).unwrap();
}

pub fn run(license: PendId) {
    let UnitTarget::Task(root) = inspect(UnitQuery::Caller).target else {
        panic!("caller")
    };
    let space = crate::space();
    let Target::Nest(nest) = info(space).target else {
        panic!("space")
    };
    let mut memory = Memory::new(|why| panic!("{why}"));
    let Value::CreatedFlat { token, flat, .. } = change(&mut memory, |_| Action::CreateFlat {
        license,
        count: 8,
        level: CellLevel::BASE,
    }) else {
        panic!("Flat")
    };
    let (memo, address) = reserve(&mut memory, space, nest, 8, 6);
    for page in 0..8 {
        let slot = SlotPosition::new(token, flat, page).unwrap();
        change(&mut memory, |memory| {
            let Snapshot::Slot(status) = memory.inspect(Query::Slot(slot)) else {
                panic!("Slot")
            };
            Action::Occupy {
                slot,
                expected: status.version,
            }
        });
        let Snapshot::Slot(status) = memory.inspect(Query::Slot(slot)) else {
            panic!("Slot")
        };
        let target = MemoPosition::new(memo.token(), nest, memo.reservation(), page).unwrap();
        publish(
            &mut memory,
            target,
            Source::Flat {
                position: slot,
                expected: status.version,
            },
        );
        // SAFETY: this Task owns the settled private RW reservation and Flat.
        unsafe {
            (address.get() as *mut u8)
                .add(page * PAGE_SIZE)
                .write_bytes(page as u8 + 1, PAGE_SIZE);
        }
    }
    let maximum = Limits {
        operations: Operations::Share(ShareRights::ASSOCIATE),
        modifiable: Modifiable::No,
        distributable: Distributable::No,
        scope: PendScope::Pages(PendRange::pages(1, 3).unwrap()),
    };
    let share = share_view::derive(
        license,
        addr(
            &AccordRequest {
                src: token,
                target: AccordTarget::Caller,
                mark: Mark::NONE,
                limits: Limits {
                    distributable: Distributable::Yes,
                    scope: PendScope::Pages(PendRange::Inherit),
                    ..maximum
                },
            }
            .to_bytes(),
        ),
    )
    .unwrap();
    release(&mut memory, memo);
    let mut source = Leases::new("system/window").unwrap();
    source
        .register(Resource {
            name: "page",
            source: share,
            maximum,
            caller: root,
            target: None,
            description: b"window",
        })
        .unwrap();
    let requested = Limits {
        scope: PendScope::Pages(PendRange::pages(1, 1).unwrap()),
        ..maximum
    };
    let id = api::LeaseId::new(1).unwrap();
    source
        .acquire(
            root,
            api::Acquire {
                lease: id,
                target: system_protocol::TaskId::new(root.get() as u64),
                binding: api::Binding {
                    source: api::Key {
                        name: String::from("system/window"),
                    },
                    resource: api::Key {
                        name: String::from("page"),
                    },
                    slot: system_protocol::Mark::new(92),
                    rights: Rights(requested),
                },
            },
        )
        .unwrap();
    let deadline = room::clock() + 5_000_000_000;
    let delivered = loop {
        source.advance().unwrap();
        let reply = source.inspect(root, id).unwrap();
        if reply.state == api::State::Granted {
            break PendId::try_from(reply.delivery.0.unwrap().token).unwrap();
        }
        assert!(room::clock() < deadline);
    };
    let delivered_info = info(delivered);
    assert_eq!(
        delivered_info.limits.scope,
        PendScope::Pages(PendRange::pages(2, 1).unwrap())
    );
    let baseline = crate::pends();
    for rounds in 0..=4 {
        // SAFETY: trusted same-Task Space; the private read projection is not
        // transferred to any other Task and keeps its borrowed source alive.
        let mut read =
            unsafe { crate::runtime::view::Read::new(space, delivered, |why| panic!("{why}")) }
                .unwrap();
        for _ in 0..rounds {
            advance(&mut read);
        }
        let deadline = room::clock() + 5_000_000_000;
        while !finish(&mut read) {
            assert!(room::clock() < deadline);
        }
        assert_eq!(
            crate::pends(),
            baseline,
            "partial view cleanup retained a Pend"
        );
        assert_eq!(
            info(delivered).status,
            kernel_api::model::pend::PendStatus::Active
        );
    }
    let mut read =
        unsafe { crate::runtime::view::Read::new(space, delivered, |why| panic!("{why}")) }
            .unwrap();
    let deadline = room::clock() + 5_000_000_000;
    while !advance(&mut read) {
        assert!(room::clock() < deadline);
    }
    // SAFETY: our borrowed source is immutable throughout this access and no
    // other owner can replace this private, settled read-only reservation.
    let bytes = unsafe { read.bytes() }.unwrap();
    assert_eq!(bytes.len(), PAGE_SIZE);
    assert_eq!(bytes[0], 3);
    assert_eq!(bytes[PAGE_SIZE - 1], 3);
    while !finish(&mut read) {
        assert!(room::clock() < deadline);
    }
    assert_eq!(crate::pends(), baseline);
    source.release(root, id).unwrap();
    let deadline = room::clock() + 5_000_000_000;
    while !source.finish().unwrap() {
        assert!(room::clock() < deadline);
        room::yield_();
    }
    pend::return_(share).unwrap();
    change(&mut memory, |memory| {
        let Snapshot::Flat(status) = memory.inspect(Query::Flat { token, flat }) else {
            panic!("Flat")
        };
        Action::DeleteFlat {
            token,
            flat,
            expected: status.version,
        }
    });
    pend::return_(token).unwrap();
}

fn advance(read: &mut crate::runtime::view::Read) -> bool {
    match read.advance() {
        Ok(ready) => ready,
        Err(error) if error.busy() => false,
        Err(error) => panic!("view open: {error:?}"),
    }
}
fn finish(read: &mut crate::runtime::view::Read) -> bool {
    match read.finish() {
        Ok(done) => done,
        Err(error) if error.busy() => false,
        Err(error) => panic!("view retirement: {error:?}"),
    }
}

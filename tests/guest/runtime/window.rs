//! Relative windows are resolved once before recipient delivery.
use crate::{
    addr, inspect,
    native_supply::{Resource, Source},
};
use alloc::string::String;
use kernel_api::{
    address::{PAGE_SIZE, VirtAddr},
    model::{
        cell::{
            Allocation, CellAssociation, CellCreate, CellInfo, CellObject, CellPosition,
            CellSource, CellState, CreatedCell, MemoDestination, Placement, ReferenceRange,
        },
        identity::{PendId, TeamId},
        modifiable::Modifiable,
        pend::{Distributable, Limits, Operations, PendRange, ShareRights},
        unit::{UnitQuery, UnitTarget},
    },
};
use kernel_client::call::{cell, pend, room, share_view};
use system_protocol::{supply as api, value::Rights};
fn position(object: CellObject, page: usize) -> CellPosition {
    CellPosition {
        object,
        page,
        generation: 0,
    }
}
pub fn run(license: PendId) {
    let UnitTarget::Task(root) = inspect(UnitQuery::Caller).target else {
        panic!("caller");
    };
    let CreatedCell::Slot(slot) = cell::create(
        license,
        addr(&CellCreate::Slot(Allocation::new(8 * PAGE_SIZE, PAGE_SIZE).unwrap()).to_bytes()),
    )
    .unwrap() else {
        panic!("Slot");
    };
    let descriptor = CellCreate::Memo {
        source: CellSource::Slot(ReferenceRange {
            reference: slot,
            offset: 0,
            count: 8,
        }),
        destination: MemoDestination::Internal {
            team: TeamId::new(0),
            placement: Placement::lowest(PAGE_SIZE).unwrap(),
            access: 6,
        },
    }
    .to_bytes();
    let CreatedCell::Memo {
        token: memo,
        address,
    } = cell::create(PendId::NONE, addr(&descriptor)).unwrap()
    else {
        panic!("Memo");
    };
    for page in 0..8 {
        let deadline = room::clock() + 5_000_000_000;
        loop {
            let mut info = [0; CellInfo::WIDTH];
            let mut target = position(CellObject::Memo(memo), page);
            cell::inspect(
                addr(&target.to_bytes()),
                VirtAddr::new(info.as_mut_ptr().addr()),
            )
            .unwrap();
            let state = CellInfo::from_bytes(&info).unwrap();
            target.generation = state.generation;
            match state.state {
                CellState::Vacant => cell::insert(addr(
                    &CellAssociation {
                        source: position(CellObject::Slot(slot), page),
                        target,
                    }
                    .to_bytes(),
                ))
                .unwrap(),
                CellState::Ready => cell::publish(addr(&target.to_bytes())).unwrap(),
                CellState::Published => break,
                CellState::Publishing | CellState::Retracting => room::yield_(),
                CellState::Failed => panic!("mapping"),
            }
            assert!(room::clock() < deadline);
        }
        // SAFETY: this Task owns the private published RW Memo and full Slot.
        unsafe {
            (address.get() as *mut u8)
                .add(page * PAGE_SIZE)
                .write_bytes(page as u8 + 1, PAGE_SIZE);
        }
    }
    let share = share_view::create(license, slot, Modifiable::No).unwrap();
    loop {
        match pend::return_(memo) {
            Ok(()) => break,
            Err(error) if error.source.is_busy() => room::yield_(),
            Err(error) => panic!("{error:?}"),
        }
    }
    let maximum = Limits {
        operations: Operations::Share(ShareRights::OPEN | ShareRights::SHUT),
        modifiable: Modifiable::No,
        distributable: Distributable::No,
        range: PendRange::pages(1, 3).unwrap(),
    };
    let mut source = Source::new("system/window").unwrap();
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
    let mut requested = maximum;
    requested.range = PendRange::pages(1, 1).unwrap();
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
    let (view, span) = share_view::open(delivered).unwrap();
    assert_eq!(span, PAGE_SIZE);
    // SAFETY: our active read-only borrow covers this exact page until Shut.
    assert_eq!(unsafe { (view.get() as *const u8).read_volatile() }, 3);
    share_view::shut(delivered).unwrap();
    source.release(root, id).unwrap();
    let deadline = room::clock() + 5_000_000_000;
    while !source.finish().unwrap() {
        assert!(room::clock() < deadline);
        room::yield_();
    }
    loop {
        match pend::return_(share) {
            Ok(()) => break,
            Err(error) if error.source.is_busy() => room::yield_(),
            Err(error) => panic!("{error:?}"),
        }
    }
    cell::delete(addr(&CellObject::Slot(slot).to_bytes())).unwrap();
}

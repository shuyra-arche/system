//! Kernel boot data must be native read-only RAM, not an unauthorised region.
use crate::{
    boot::{Accounts, Resource},
    runtime::{memory::Memory, view::Read},
};
use kernel_api::{
    abi::{
        call::pend::PendFail,
        memory::{Action, Failure, Query, Snapshot, Source, Value},
    },
    address::{PAGE_SIZE, PageCount, VirtAddr},
    boot::ledger::{Name, Page},
    model::{
        cell::{CellLevel, Placement},
        identity::{Mark, PendId},
        modifiable::Modifiable,
        nest::{MemoPosition, ReserveRequest},
        pend::{
            AccordRequest, AccordTarget, Distributable, Operations, PendInfo, PendRange, PendScope,
            ShareRights, Target, ViewTarget,
        },
    },
};
use kernel_client::call::{pend, room};

pub fn run(accounts: &Accounts<'_>) {
    let Resource::Nest(space) = accounts.resource(Name::Space).unwrap() else {
        panic!("Space")
    };
    for page in [Page::Dtb, Page::Initrd] {
        let source = accounts.page(page).unwrap();
        let mut bytes = [0; PendInfo::SIZE];
        pend::inspect(source, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap();
        let info = PendInfo::from_bytes(&bytes).unwrap();
        assert_eq!(info.limits.modifiable, Modifiable::No);
        assert!(
            info.limits
                .operations
                .contains(Operations::Share(ShareRights::ASSOCIATE))
        );
        let mut broader = info.limits;
        broader.modifiable = Modifiable::Yes;
        broader.distributable = Distributable::No;
        let request = AccordRequest {
            src: source,
            target: AccordTarget::Caller,
            mark: Mark::NONE,
            limits: broader,
        }
        .to_bytes();
        assert_eq!(
            pend::accord(VirtAddr::new(request.as_ptr().addr()))
                .unwrap_err()
                .source,
            PendFail::Denied
        );
        deny_write(space, &info);
    }
    let source = accounts.page(Page::Initrd).unwrap();
    let baseline = pends();
    // SAFETY: grants are this initial Task's trusted Space and immutable initrd.
    let mut read = unsafe { Read::new(space, source, |why| panic!("{why}")) }.unwrap();
    let deadline = room::clock() + 20_000_000_000;
    loop {
        match read.advance() {
            Ok(true) => break,
            Ok(false) => (),
            Err(error) if error.busy() => (),
            Err(error) => panic!("boot data open: {error:?}"),
        }
        assert!(room::clock() < deadline);
    }
    // SAFETY: Kernel retains these immutable boot bytes. This private projection
    // remains published until the comparison finishes and before explicit close.
    let bytes = unsafe { read.bytes() }.unwrap();
    assert_eq!(&bytes[..accounts.view().len()], accounts.view());
    assert!(bytes[accounts.view().len()..].iter().all(|byte| *byte == 0));
    loop {
        match read.finish() {
            Ok(true) => break,
            Ok(false) => (),
            Err(error) if error.busy() => (),
            Err(error) => panic!("boot data retirement: {error:?}"),
        }
        assert!(room::clock() < deadline);
    }
    assert_eq!(pends(), baseline);
    system_support::debug::put("PASS boot-data native=1 readonly=1 content=1 padding=1 retire=1\n");
}

fn deny_write(space: PendId, source: &PendInfo) {
    let baseline = pends();
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(space, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap();
    let Target::Nest(nest) = PendInfo::from_bytes(&bytes).unwrap().target else {
        panic!("Nest")
    };
    let Target::View(ViewTarget::Share(view)) = source.target else {
        panic!("Share")
    };
    let PendScope::Pages(PendRange::Pages { offset, .. }) = source.limits.scope else {
        panic!("Pages")
    };
    let mut memory = Memory::new(|why| panic!("{why}"));
    let Snapshot::Nest(status) = memory.inspect(Query::Nest { token: space, nest }) else {
        panic!("Nest snapshot")
    };
    let Value::Reservation {
        token, reservation, ..
    } = memory
        .mutate(Action::Reserve {
            token: space,
            nest,
            expected: status.version,
            reservation: ReserveRequest {
                placement: Placement::lowest(PAGE_SIZE).unwrap(),
                count: PageCount::new(1).unwrap(),
                level: CellLevel::BASE,
                access: 6,
            },
        })
        .unwrap()
    else {
        panic!("Reservation")
    };
    let target = MemoPosition::new(token, nest, reservation, 0).unwrap();
    let Snapshot::Memo(status) = memory.inspect(Query::Memo(target)) else {
        panic!("Memo snapshot")
    };
    memory
        .mutate(Action::Insert {
            target,
            expected: status.generation,
            source: Source::Share {
                token: source.token,
                view,
                page: offset,
                count: 1,
            },
        })
        .unwrap();
    let Snapshot::Memo(status) = memory.inspect(Query::Memo(target)) else {
        panic!("Memo snapshot after association")
    };
    assert_eq!(
        memory.mutate(Action::Publish {
            memo: target,
            expected: status.generation,
            maintenance: None
        }),
        Err(Failure::Denied)
    );
    let Snapshot::Reservation(status) = memory.inspect(Query::Reservation {
        token,
        nest,
        reservation,
    }) else {
        panic!("Reservation snapshot")
    };
    memory
        .mutate(Action::Release {
            token,
            nest,
            reservation,
            expected: status.version,
        })
        .unwrap();
    pend::return_(token).unwrap();
    assert_eq!(pends(), baseline);
}
fn pends() -> usize {
    let mut after = PendId::NONE;
    let mut count = 0;
    loop {
        let mut bytes = [0; PendInfo::SIZE];
        if pend::collect(after, VirtAddr::new(bytes.as_mut_ptr().addr()), 1).unwrap() == 0 {
            return count;
        }
        after = PendInfo::from_bytes(&bytes).unwrap().token;
        count += 1;
    }
}

//! Real internal-service bootstrap using the production heap and native entry.
use kernel_api::model::identity::PendId;
use kernel_client::call::room;
pub fn run(license: PendId, space: PendId) {
    let mut bytes = [0; kernel_api::model::pend::PendInfo::SIZE];
    kernel_client::call::pend::inspect(
        space,
        kernel_api::address::VirtAddr::new(bytes.as_mut_ptr().addr()),
    )
    .unwrap();
    let kernel_api::model::pend::Target::Nest(nest) =
        kernel_api::model::pend::PendInfo::from_bytes(&bytes)
            .unwrap()
            .target
    else {
        panic!("Internal Nest");
    };
    // SAFETY: the trusted boot ledger supplies the current Internal Nest. This
    // context remains in ROOT and shares its heap's native request allocator.
    let memory = unsafe {
        twine::view::memory::Native::new(
            space,
            nest,
            crate::runtime::memory::next_request as fn() -> _,
        )
    };
    let context = system_client::Context::new(memory);
    let mut workers = crate::supervisor_boot::Workers::new(license, context).unwrap();
    if cfg!(feature = "stall") {
        stall(&mut workers);
    }
    let deadline = room::clock() + 10_000_000_000;
    loop {
        crate::runtime::heap::poll();
        if workers.advance().unwrap() {
            break;
        }
        assert!(room::clock() < deadline, "service startup");
        room::yield_();
    }
    let publication = workers.publication().unwrap();
    assert_ne!(publication.operator, publication.identity);
    assert!(
        workers
            .identity_face(system_protocol::identity::Grant::Resolve)
            .is_some()
    );
    let deadline = room::clock() + 10_000_000_000;
    loop {
        crate::runtime::heap::poll();
        if workers.finish().unwrap() {
            break;
        }
        assert!(room::clock() < deadline, "service retirement");
        room::yield_();
    }
    assert_eq!(workers.phase(), crate::supervisor_boot::Phase::Closed);
    drop(workers);
    let allocation = alloc::vec![0xa5u8; 3 * 1024 * 1024];
    assert_eq!(allocation[allocation.len() - 1], 0xa5);
    drop(allocation);
    system_support::debug::put(
        "PASS entry-services heap=production roles=2 root=1 wired=1 retire=1\n",
    );
}

fn stall(workers: &mut crate::supervisor_boot::Workers) -> ! {
    use kernel_api::{
        address::VirtAddr,
        model::{
            identity::TaskId,
            unit::{UnitInfo, UnitQuery},
        },
    };
    use kernel_client::call::unit;
    while workers.phase() == crate::supervisor_boot::Phase::Preparing {
        crate::runtime::heap::poll();
        assert!(!workers.advance().unwrap());
    }
    assert_eq!(workers.phase(), crate::supervisor_boot::Phase::Operator);
    let mut bytes = [0; UnitInfo::WIDTH];
    assert!(unit::inspect(UnitQuery::Caller, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap());
    let caller = UnitInfo::from_bytes(&bytes).unwrap();
    let mut after = TaskId::new(0);
    let worker = loop {
        assert!(
            unit::inspect(
                UnitQuery::Members {
                    team: caller.team,
                    after
                },
                VirtAddr::new(bytes.as_mut_ptr().addr())
            )
            .unwrap()
        );
        let member = UnitInfo::from_bytes(&bytes).unwrap();
        let kernel_api::model::unit::UnitTarget::Task(task) = member.target else {
            panic!("member")
        };
        after = task;
        if member.target != caller.target {
            break member.target;
        }
    };
    unit::freeze(worker).unwrap();
    let deadline = room::clock().saturating_add(6_000_000_000);
    loop {
        crate::runtime::heap::poll();
        match workers.advance() {
            Err(crate::supervisor_boot::Error::TimedOut(
                crate::supervisor_boot::Phase::Operator,
            )) => break,
            Ok(false) => (),
            other => panic!("unexpected stalled bootstrap: {other:?}"),
        }
        assert!(
            room::clock() < deadline,
            "polling renewed a stalled bootstrap"
        );
        room::yield_();
    }
    system_support::debug::put("EXPECTED entry-stall timed-out=1\n");
    crate::runtime::termination::abort(43, "system: stalled entry bootstrap")
}

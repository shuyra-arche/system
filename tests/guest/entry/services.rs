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

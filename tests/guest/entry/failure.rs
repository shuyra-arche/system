//! Fatal root exit while two workers still read root-owned heap backing.
use alloc::boxed::Box;
use core::sync::atomic::{AtomicUsize, Ordering};
use kernel_client::call::room;
static READY: AtomicUsize = AtomicUsize::new(0);

pub fn run() -> ! {
    let backing = Box::new(0x7359_627au64);
    let pointer = (&*backing as *const u64).addr();
    // The Box and administrative records stay live on this root's stack until
    // production termination proves that neither worker can still touch them.
    let mut workers = core::array::from_fn::<_, 2, _>(|_| unsafe {
        crate::worker::Worker::prepare(reader, &[pointer], 65536).unwrap()
    });
    for worker in &mut workers {
        worker.resume().unwrap();
    }
    let deadline = room::clock().saturating_add(5_000_000_000);
    while READY.load(Ordering::Acquire) != 2 {
        crate::runtime::heap::poll();
        assert!(room::clock() < deadline);
        room::yield_();
    }
    system_support::debug::put("EXPECTED entry-abort workers=2 backing=live\n");
    crate::runtime::termination::abort(42, "system: injected entry failure")
}
unsafe extern "C" fn reader(words: *const usize, count: usize) -> ! {
    assert_eq!(count, 1);
    // Kernel copied these argument words; root retains the pointed-to Box.
    let pointer = unsafe { *words } as *const u64;
    READY.fetch_add(1, Ordering::Release);
    loop {
        assert_eq!(unsafe { core::ptr::read_volatile(pointer) }, 0x7359_627a);
        room::yield_();
    }
}

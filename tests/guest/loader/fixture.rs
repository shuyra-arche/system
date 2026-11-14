use core::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};
use kernel_api::{
    address::VirtAddr,
    boot::{
        args,
        ledger::{Call, Entries, Name},
    },
    model::{identity::PendId, pend::PendInfo, unit::ExitScope},
};
use kernel_client::call::{debug, pend, room, unit};
static LICENSE: AtomicUsize = AtomicUsize::new(0);
static SPACE: AtomicUsize = AtomicUsize::new(0);
pub fn boot(at: usize, count: usize) {
    assert_eq!(count, 4);
    // SAFETY: Kernel starts this initial Task with four trusted mapped words.
    let words = unsafe { core::slice::from_raw_parts(at as *const usize, count) };
    // SAFETY: the trusted initial-Task ledger mapping is readable for this Team.
    let bytes = unsafe {
        core::slice::from_raw_parts(words[args::LEDGER] as *const u8, words[args::LEDGER_LEN])
    };
    let ledger = Entries::new(bytes).unwrap();
    let license = PendId::mint(ledger.find(Name::Call(Call::Create)).unwrap().handle());
    let space = PendId::mint(ledger.find(Name::Space).unwrap().handle());
    LICENSE.store(license.get(), Ordering::Release);
    SPACE.store(space.get(), Ordering::Release);
    // SAFETY: only this initial Task initializes the allocator, using its
    // genuine creation license and actual current Space from the boot ledger.
    unsafe { crate::native_heap::initialize(license, space) }.unwrap();
}
pub fn license() -> PendId {
    PendId::mint(LICENSE.load(Ordering::Acquire))
}
pub fn space() -> PendId {
    PendId::mint(SPACE.load(Ordering::Acquire))
}
pub fn exit(code: u64) -> ! {
    unit::exit(code, 0, VirtAddr::new(0), 0, ExitScope::Team)
}
pub fn until(mut progress: impl FnMut() -> bool) {
    let deadline = room::clock().saturating_add(20_000_000_000);
    while !progress() {
        assert!(room::clock() < deadline, "Loader progress timed out");
        crate::native_heap::poll();
        room::yield_();
    }
}
pub fn active() -> usize {
    // Talc may return backing after the scenario; settle broker ownership before
    // comparing the full caller-local Pend inventory, not just active entries.
    crate::native_heap::poll();
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
pub struct Console;
impl fmt::Write for Console {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            let used = debug::put(VirtAddr::new(bytes.as_ptr().addr()), bytes.len().min(256))
                .map_err(|_| fmt::Error)?;
            bytes = &bytes[used..];
            if used == 0 {
                room::yield_();
            }
        }
        Ok(())
    }
}

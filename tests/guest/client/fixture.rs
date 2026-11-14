use core::{
    alloc::{GlobalAlloc, Layout},
    fmt, ptr,
    sync::atomic::{AtomicUsize, Ordering},
};
use kernel_api::{
    address::VirtAddr,
    model::{
        identity::{Mark, PendId, TaskId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, Distributable, Limits, Operations, PendInfo, PendRange,
            PendScope, PendStatus, Target,
        },
        unit::{ExitScope, UnitCreate, UnitInfo, UnitQuery, UnitTarget},
    },
};
use kernel_client::call::{debug, pend, room, unit};

pub fn addr(bytes: &[u8]) -> VirtAddr {
    VirtAddr::new(bytes.as_ptr().addr())
}
pub fn deadline() -> u64 {
    room::clock().saturating_add(5_000_000_000)
}

pub struct Console;
impl fmt::Write for Console {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = deadline();
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            let count = debug::put(addr(bytes), bytes.len().min(256)).map_err(|_| fmt::Error)?;
            if count == 0 {
                assert!(room::clock() < end);
                room::yield_();
            }
            bytes = &bytes[count..];
        }
        Ok(())
    }
}

pub fn exit(code: u64) -> ! {
    unit::exit(code, 0, VirtAddr::new(0), 0, ExitScope::Task)
}

pub fn inspect(query: UnitQuery) -> UnitInfo {
    let mut bytes = [0; UnitInfo::WIDTH];
    assert!(unit::inspect(query, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap());
    UnitInfo::from_bytes(&bytes).unwrap()
}

pub fn license() -> PendId {
    let mut after = PendId::NONE;
    loop {
        let mut words = [0; PendInfo::WORDS];
        assert_eq!(
            pend::collect(after, VirtAddr::new(words.as_mut_ptr().addr()), 1).unwrap(),
            1
        );
        let info = PendInfo::from_words(words).unwrap();
        if info.target == Target::None {
            return info.token;
        }
        after = info.token;
    }
}

pub fn active() -> usize {
    let mut after = PendId::NONE;
    let mut count = 0;
    loop {
        let mut words = [0; PendInfo::WORDS];
        if pend::collect(after, VirtAddr::new(words.as_mut_ptr().addr()), 1).unwrap() == 0 {
            return count;
        }
        let info = PendInfo::from_words(words).unwrap();
        if info.status == PendStatus::Active {
            count += 1;
        }
        after = info.token;
    }
}

pub fn owned_reply(owner: TaskId) -> PendId {
    let mut after = PendId::NONE;
    loop {
        let mut words = [0; PendInfo::WORDS];
        assert_eq!(
            pend::collect(after, VirtAddr::new(words.as_mut_ptr().addr()), 1).unwrap(),
            1
        );
        let info = PendInfo::from_words(words).unwrap();
        if info.owner == owner
            && matches!(
                info.target,
                Target::View(kernel_api::model::pend::ViewTarget::Clone(_))
            )
        {
            return info.token;
        }
        after = info.token;
    }
}

pub fn grant(
    src: PendId,
    target: TaskId,
    operations: Operations,
    modifiable: Modifiable,
) -> PendId {
    let request = AccordRequest {
        src,
        target: AccordTarget::Task(target),
        mark: Mark::NONE,
        limits: Limits {
            operations,
            modifiable,
            distributable: Distributable::No,
            scope: match operations {
                Operations::Share(_) | Operations::Flat(_) => PendScope::Pages(PendRange::Inherit),
                _ => PendScope::Object,
            },
        },
    };
    pend::accord(addr(&request.to_bytes())).unwrap()
}

pub fn spawn(entry: extern "C" fn() -> !) -> (UnitTarget, TaskId) {
    let request = UnitCreate::Task {
        team: None,
        entry: VirtAddr::new(entry as *const () as usize),
        args: VirtAddr::new(0),
        count: 0,
        stack: VirtAddr::new(128 * 1024),
    };
    let created = unit::create(addr(&request.to_bytes())).unwrap();
    let kernel_api::model::unit::CreatedUnit::Task(task) = created else {
        panic!("expected Task")
    };
    (UnitTarget::Task(task), task)
}

pub fn until(mut done: impl FnMut() -> bool) {
    let end = deadline();
    while !done() {
        assert!(room::clock() < end, "condition timed out");
        room::yield_();
    }
}

// This guest has a fixed memory budget. Allocation failures stay observable and
// deallocation is intentionally unnecessary for its bounded number of calls.
const HEAP_SIZE: usize = 2 * 1024 * 1024;
#[repr(align(4096))]
struct Heap([u8; HEAP_SIZE]);
static mut HEAP: Heap = Heap([0; HEAP_SIZE]);
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Allocator;
// SAFETY: Atomic reservation assigns disjoint in-bounds ranges to all Tasks.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let result = NEXT.try_update(Ordering::AcqRel, Ordering::Acquire, |next| {
            next.checked_add(layout.align() - 1)
                .map(|n| n & !(layout.align() - 1))
                .and_then(|start| start.checked_add(layout.size()))
                .filter(|end| *end <= HEAP_SIZE)
        });
        match result {
            Ok(next) => {
                let start = (next + layout.align() - 1) & !(layout.align() - 1);
                // SAFETY: try_update reserved these bytes exclusively.
                unsafe { ptr::addr_of_mut!(HEAP.0).cast::<u8>().add(start) }
            }
            Err(_) => ptr::null_mut(),
        }
    }
    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

static SPACE: AtomicUsize = AtomicUsize::new(0);
pub fn boot(at: usize, count: usize) {
    use kernel_api::boot::{
        args,
        ledger::{Entries, Name},
    };
    assert_eq!(count, args::LEN);
    // SAFETY: the initial Task receives this validated startup argument block.
    let words = unsafe { core::slice::from_raw_parts(at as *const usize, count) };
    // SAFETY: the trusted boot arguments identify the read-only Ledger mapping.
    let bytes = unsafe {
        core::slice::from_raw_parts(words[args::LEDGER] as *const u8, words[args::LEDGER_LEN])
    };
    let entry = Entries::new(bytes).unwrap().find(Name::Space).unwrap();
    SPACE.store(entry.handle(), Ordering::Release);
}
pub fn space() -> PendId {
    let raw = SPACE.load(Ordering::Acquire);
    assert_ne!(raw, 0);
    PendId::from_bytes(&(raw as u64).to_le_bytes()).unwrap()
}

// A request deadline never restarts. This separate fixed bound covers cleanup
// after success, timeout or rejection while preserving its actual owner.
pub fn cleanup(mut operation: impl FnMut() -> Result<(), system_client::CleanupError>) {
    until(|| match operation() {
        Ok(()) => true,
        Err(error) if error.is_busy() => false,
        Err(error) => panic!("client cleanup {error:?}"),
    });
}

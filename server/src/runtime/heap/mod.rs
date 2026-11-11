//! Team heap backed by Memos owned by the initial System Task.
//! The initial supervisor must poll between finite rounds, including shutdown.
mod lock;
mod pages;
use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicU32, AtomicUsize, Ordering::*},
};
use kernel_api::{
    address::VirtAddr,
    model::{
        identity::{PendId, TaskId},
        pend::{CreationRights, Operations, PendInfo, PendStatus},
        unit::{ExitScope, UnitInfo, UnitQuery, UnitTarget},
    },
};
use kernel_client::call::{pend, room, unit};
use talc::{TalcLock, source::GlobalAllocSource};
const IDLE: u32 = 0;
const WRITING: u32 = 1;
const REQUESTED: u32 = 2;
const ANSWERED: u32 = 3;
const BLOCK: usize = 4 * kernel_api::address::PAGE_SIZE;
static ROOT: AtomicUsize = AtomicUsize::new(0);
// Only ROOT reconstructs this table-local Pend from raw storage.
static LICENSE: AtomicUsize = AtomicUsize::new(0);
static MAPPINGS: AtomicUsize = AtomicUsize::new(0);
#[derive(Clone, Copy)]
enum Request {
    Allocate { size: usize, align: usize },
    Free { address: usize },
}
struct Mailbox {
    state: AtomicU32,
    request: UnsafeCell<Request>,
    answer: UnsafeCell<usize>,
}
// SAFETY: one requester holds the heap lock; release/acquire separates request,
// ROOT's effects and response. ROOT never enters Talc while serving a request.
unsafe impl Sync for Mailbox {}
static MAILBOX: Mailbox = Mailbox {
    state: AtomicU32::new(IDLE),
    request: UnsafeCell::new(Request::Free { address: 0 }),
    answer: UnsafeCell::new(0),
};

/// # Safety
/// Call once from the initial Task before allocating or starting workers. This
/// Task and its creation grant must outlive all allocations and workers.
pub unsafe fn initialize(license: PendId) -> Result<(), &'static str> {
    let root = caller().ok_or("missing initial Task")?;
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(license, VirtAddr::new(bytes.as_mut_ptr().addr())).map_err(|_| "heap license")?;
    let info = PendInfo::from_bytes(&bytes).ok_or("invalid heap license")?;
    if info.status != PendStatus::Active
        || !info
            .limits
            .operations
            .contains(Operations::None(CreationRights::FLAT))
    {
        return Err("heap creation denied");
    }
    if ROOT.load(Acquire) != 0 {
        return Err("heap already initialized");
    }
    LICENSE.store(license.get(), Relaxed);
    ROOT.store(root.get(), Release);
    Ok(())
}
pub fn poll() {
    if caller().is_some_and(|task| task.get() == ROOT.load(Acquire)) {
        service();
    }
}
pub fn mappings() -> usize {
    MAPPINGS.load(Acquire)
}
fn service() {
    if MAILBOX.state.load(Acquire) != REQUESTED {
        return;
    }
    // SAFETY: REQUESTED publishes a complete request; the worker retains the
    // heap lock until it reads ROOT's release-published answer.
    let answer = perform(unsafe { MAILBOX.request.get().read() });
    unsafe { MAILBOX.answer.get().write(answer) };
    MAILBOX.state.store(ANSWERED, Release);
}
fn perform(value: Request) -> usize {
    let raw = LICENSE.load(Acquire);
    if raw == 0 {
        fatal();
    }
    let license = PendId::from_bytes(&(raw as u64).to_le_bytes()).unwrap_or_else(|| fatal());
    match value {
        Request::Allocate { size, align } => pages::allocate(license, size, align),
        Request::Free { address } => {
            pages::free(address);
            0
        }
    }
}
fn request(value: Request) -> usize {
    let task = caller().unwrap_or_else(|| fatal());
    let root = ROOT.load(Acquire);
    if root == 0 {
        return 0;
    }
    if root == task.get() {
        return perform(value);
    }
    // This private source is called only with Talc's exclusive heap lock.
    if MAILBOX
        .state
        .compare_exchange(IDLE, WRITING, Acquire, Relaxed)
        .is_err()
    {
        fatal();
    }
    unsafe { MAILBOX.request.get().write(value) };
    MAILBOX.state.store(REQUESTED, Release);
    while MAILBOX.state.load(Acquire) != ANSWERED {
        room::yield_();
    }
    let answer = unsafe { MAILBOX.answer.get().read() };
    MAILBOX.state.store(IDLE, Release);
    answer
}
fn caller() -> Option<TaskId> {
    match inspect(UnitQuery::Caller)?.target {
        UnitTarget::Task(task) => Some(task),
        _ => None,
    }
}
fn inspect(query: UnitQuery) -> Option<UnitInfo> {
    let mut bytes = [0; UnitInfo::WIDTH];
    loop {
        match unit::inspect(query, VirtAddr::new(bytes.as_mut_ptr().addr())) {
            Ok(true) => break,
            Err(error) if error.source.is_busy() => room::yield_(),
            _ => return None,
        }
    }
    UnitInfo::from_bytes(&bytes)
}
fn address<T>(value: &T) -> VirtAddr {
    VirtAddr::new(value as *const T as usize)
}
fn fatal() -> ! {
    let note = b"system: heap ownership failed";
    unit::exit(
        0,
        kernel_api::model::exit::EXIT_PANIC as u64,
        VirtAddr::new(note.as_ptr().addr()),
        note.len(),
        ExitScope::Team,
    )
}
#[derive(Debug)]
struct Pages;
// SAFETY: Talc serializes calls. ROOT owns and mutates every backing Memo.
unsafe impl GlobalAlloc for Pages {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        request(Request::Allocate {
            size: layout.size(),
            align: layout.align(),
        }) as *mut u8
    }
    unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
        request(Request::Free {
            address: ptr.addr(),
        });
    }
}
static HEAP: TalcLock<lock::Lock, GlobalAllocSource<Pages>> =
    TalcLock::new(GlobalAllocSource::with_block_size(Pages, BLOCK));
pub struct Allocator;
// SAFETY: Talc owns allocation metadata under Lock; ROOT services backing
// growth/free. An allocating worker never holds backing-memory authority.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ROOT.load(Acquire) == 0 || !supported(layout) {
            return ptr::null_mut();
        }
        unsafe { HEAP.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { HEAP.dealloc(ptr, layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if ROOT.load(Acquire) == 0 || !supported(layout) {
            return ptr::null_mut();
        }
        unsafe { HEAP.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let Ok(next) = Layout::from_size_align(size, layout.align()) else {
            return ptr::null_mut();
        };
        if !supported(next) {
            return ptr::null_mut();
        }
        unsafe { HEAP.realloc(ptr, layout, size) }
    }
}
fn supported(layout: Layout) -> bool {
    layout
        .size()
        .checked_add(layout.align())
        .and_then(|n| n.checked_add(talc::min_first_heap_layout::<talc::DefaultBinning>().size()))
        .and_then(|n| n.checked_add(2 * BLOCK))
        .is_some_and(|n| n <= isize::MAX as usize)
}

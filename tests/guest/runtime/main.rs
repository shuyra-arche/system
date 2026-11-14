#![no_std]
#![no_main]
extern crate alloc;
#[path = "../../../server/src/runtime/heap/mod.rs"]
mod heap;
#[path = "../../../server/src/runtime/memory.rs"]
pub mod memory;
pub mod runtime {
    pub use crate::memory;
}
#[path = "../../../server/src/system/supply/native.rs"]
mod native_supply;
#[path = "../../../server/src/protocol/mod.rs"]
pub mod protocol;
mod rpc;
mod supply;
mod window;
#[path = "../../../server/src/system/app/worker/mod.rs"]
pub mod worker;
use alloc::{boxed::Box, vec, vec::Vec};
use core::{
    fmt::{self, Write},
    sync::atomic::{AtomicUsize, Ordering::*},
};
use kernel_api::{
    address::VirtAddr,
    model::{
        identity::{PendId, TaskId},
        pend::{PendInfo, Target},
        unit::{CreatedUnit, ExitScope, UnitCreate, UnitInfo, UnitQuery, UnitState},
    },
};
use kernel_client::call::{debug, pend, room, unit};
#[global_allocator]
static ALLOCATOR: heap::Allocator = heap::Allocator;
const WORKERS: usize = 4;
const BYTES: usize = 768 * 1024;
static POINTERS: [AtomicUsize; WORKERS] = [const { AtomicUsize::new(0) }; WORKERS];
struct Console;
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
fn exit(code: u64) -> ! {
    unit::exit(code, 0, VirtAddr::new(0), 0, ExitScope::Team)
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = writeln!(Console, "FAIL system-runtime {info}");
    exit(1)
}
core::arch::global_asm!(
    ".section .text.entry",
    ".global _start",
    "_start:",
    "tail start"
);
fn addr<T>(value: &T) -> VirtAddr {
    VirtAddr::new(value as *const T as usize)
}
fn inspect(query: UnitQuery) -> UnitInfo {
    let mut bytes = [0; UnitInfo::WIDTH];
    assert!(unit::inspect(query, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap());
    UnitInfo::from_bytes(&bytes).unwrap()
}
fn license() -> PendId {
    let mut after = PendId::NONE;
    loop {
        let mut bytes = [0; PendInfo::SIZE];
        assert_eq!(
            pend::collect(after, VirtAddr::new(bytes.as_mut_ptr().addr()), 1).unwrap(),
            1
        );
        let info = PendInfo::from_bytes(&bytes).unwrap();
        if info.target == Target::None {
            return info.token;
        }
        after = info.token;
    }
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
fn space() -> PendId {
    let mut after = PendId::NONE;
    loop {
        let mut bytes = [0; PendInfo::SIZE];
        assert_eq!(
            pend::collect(after, VirtAddr::new(bytes.as_mut_ptr().addr()), 1).unwrap(),
            1
        );
        let info = PendInfo::from_bytes(&bytes).unwrap();
        if matches!(info.target, Target::Nest(_)) {
            return info.token;
        }
        after = info.token;
    }
}
#[unsafe(no_mangle)]
extern "C" fn start() -> ! {
    unsafe { heap::initialize(license(), space()) }.unwrap();
    let mut warm = Vec::<u64>::with_capacity(512);
    warm.push(0xaabb);
    let baseline = heap::mappings();
    let baseline_pends = pends();
    assert!(baseline > 0);
    let mut tasks = [TaskId::new(0); WORKERS];
    for (i, task) in tasks.iter_mut().enumerate() {
        let args = [i];
        let descriptor = UnitCreate::Task {
            team: None,
            entry: VirtAddr::new(worker as *const () as usize),
            args: addr(&args),
            count: 1,
            stack: VirtAddr::new(128 * 1024),
        }
        .to_bytes();
        let CreatedUnit::Task(id) = unit::create(addr(&descriptor)).unwrap() else {
            panic!("Task");
        };
        *task = id;
        unit::resume(id.into()).unwrap();
    }
    let deadline = room::clock() + 60_000_000_000;
    let mut reclaimed = [false; WORKERS];
    let mut contention = Vec::<u64>::new();
    while reclaimed.iter().any(|done| !*done) {
        heap::poll();
        for (i, task) in tasks.iter().enumerate() {
            if reclaimed[i] {
                continue;
            }
            let result = inspect(UnitQuery::Target((*task).into()));
            if result.state == UnitState::Exited && result.reclaimed {
                assert_eq!(result.result, Some(i as u64 + 17));
                unit::delete((*task).into()).unwrap();
                reclaimed[i] = true;
            }
        }
        // ROOT also contends for the heap while workers request growth.
        if contention.len() < 8192 {
            contention.extend_from_slice(&[1; 128]);
        }
        if room::clock() >= deadline {
            let _ = writeln!(
                Console,
                "heap mappings={} contention={}",
                heap::mappings(),
                contention.len()
            );
            for task in tasks {
                let _ = writeln!(
                    Console,
                    "worker {:?}",
                    inspect(UnitQuery::Target(task.into()))
                );
            }
            panic!("heap progress");
        }
        room::yield_();
    }
    assert!(heap::mappings() > baseline);
    for (i, pointer) in POINTERS.iter().enumerate() {
        let raw = pointer.load(Acquire);
        assert_ne!(raw, 0);
        // SAFETY: the now-reclaimed worker handed this sole Box to ROOT before
        // exiting. Its backing Memo belongs to ROOT, not the departed worker.
        let bytes =
            unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(raw as *mut u8, BYTES)) };
        assert!(bytes.iter().all(|byte| *byte == i as u8 + 1));
        drop(bytes);
    }
    drop(contention);
    assert_eq!(warm[0], 0xaabb);
    assert_eq!(
        heap::mappings(),
        baseline,
        "wholly free heaps must be returned"
    );
    let large = vec![0x5au8; 3 * 1024 * 1024];
    assert_eq!(large[large.len() - 1], 0x5a);
    drop(large);
    assert_eq!(heap::mappings(), baseline);
    assert_eq!(
        pends(),
        baseline_pends,
        "free heaps must also return their Flat and Reservation Pends"
    );
    writeln!(
        Console,
        "PASS system-runtime heap=1 growing=1 shared=1 owner=1 worker-exit=1 reclaim=1 contention=1"
    )
    .unwrap();
    supply::run(license());
    window::run(license());
    rpc::run(license(), space());
    writeln!(
        Console,
        "PASS supply-window relative=1 narrowed=1 once=1 cleanup=1"
    )
    .unwrap();
    exit(0)
}
extern "C" fn worker(args: *const usize, count: usize) -> ! {
    assert_eq!(count, 1);
    let i = unsafe { args.read() };
    let allocation = vec![i as u8 + 1; BYTES].into_boxed_slice();
    let raw = Box::into_raw(allocation) as *mut u8;
    POINTERS[i].store(raw.addr(), Release);
    unit::exit(i as u64 + 17, 0, VirtAddr::new(0), 0, ExitScope::Task)
}

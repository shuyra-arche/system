//! Actual native Loader on production heap, with independent ordinary User Teams.
#![no_std]
#![no_main]
extern crate alloc;
#[path = "../../../server/src/runtime/heap/mod.rs"]
pub mod native_heap;
#[path = "../../../server/src/runtime/memory.rs"]
pub mod native_memory;
#[path = "../../../server/src/runtime/view.rs"]
pub(crate) mod native_view;
pub mod runtime {
    pub(crate) use crate::native_view as view;
    pub use crate::{native_heap as heap, native_memory as memory};
}
mod fixture;
#[path = "../../../server/src/system/loader/mod.rs"]
mod native_loader;
#[path = "../client/loader.rs"]
mod scenarios;
use core::fmt::Write;
#[global_allocator]
static ALLOCATOR: native_heap::Allocator = native_heap::Allocator;
core::arch::global_asm!(
    ".section .text.entry",
    ".global _start",
    "_start:",
    "tail start"
);
#[unsafe(no_mangle)]
extern "C" fn start(at: usize, count: usize) -> ! {
    fixture::boot(at, count);
    scenarios::run(fixture::license());
    fixture::exit(0)
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = writeln!(fixture::Console, "FAIL system-loader {info}");
    fixture::exit(1)
}

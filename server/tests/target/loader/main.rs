//! Actual native Loader on production heap, with independent ordinary User Teams.
#![no_std]
#![no_main]
extern crate alloc;
#[allow(dead_code)]
#[path = "../../../src/runtime/heap/mod.rs"]
pub mod native_heap;
#[path = "../../../src/runtime/memory.rs"]
pub mod native_memory;
#[allow(dead_code)]
#[path = "../../../src/runtime/view.rs"]
pub(crate) mod native_view;
#[path = "../../../src/runtime/termination.rs"]
pub mod termination;
pub mod runtime {
    pub(crate) use crate::native_view as view;
    pub use crate::termination;
    pub use crate::{native_heap as heap, native_memory as memory};
}
mod fixture;
#[path = "../../../src/system/loader/mod.rs"]
mod native_loader;
mod cases;
#[cfg(feature = "toolchain")]
mod std_unit;
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
    cases::run(fixture::license());
    #[cfg(feature = "toolchain")]
    std_unit::run();
    fixture::exit(0)
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = writeln!(fixture::Console, "FAIL system-loader {info}");
    fixture::exit(1)
}

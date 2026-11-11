#![no_std]
#![no_main]
use kernel_api::{address::VirtAddr, model::unit::ExitScope};
static mut DATA: usize = 0x12345678;
static mut ZERO: [u8; 8192] = [0; 8192];
core::arch::global_asm!(
    ".section .text.entry,\"ax\"",
    ".global _start",
    "_start:",
    "mv a2, sp",
    "tail start"
);
fn exit(value: u64, reason: u64) -> ! {
    kernel_client::call::unit::exit(value, reason, VirtAddr::new(0), 0, ExitScope::Task)
}
#[unsafe(no_mangle)]
extern "C" fn start(words: *const usize, count: usize, stack: usize) -> ! {
    if count != 2 || stack & 15 != 0 {
        exit(0, 1);
    }
    // SAFETY: Kernel supplies copied argument words and initialized PT_LOADs.
    unsafe {
        if *words != 0xa5
            || *words.add(1) != 0x5a
            || core::ptr::read_volatile(&raw const DATA) != 0x12345678
        {
            exit(0, 2);
        }
        for i in 0..8192 {
            if core::ptr::read_volatile((&raw const ZERO).cast::<u8>().add(i)) != 0 {
                exit(0, 3);
            }
        }
        core::ptr::write_volatile(&raw mut DATA, 42);
        core::ptr::write_volatile((&raw mut ZERO).cast::<u8>().add(4096), 99);
    }
    exit(17, 0)
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    exit(0, 4)
}

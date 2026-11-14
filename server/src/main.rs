#![no_std]
#![no_main]

use core::fmt::{self, Write};
use kernel_api::{
    address::VirtAddr,
    boot::ledger::{Call, Name},
    model::unit::ExitScope,
};
use kernel_client::call::{debug, unit};

#[global_allocator]
static HEAP: system_server::runtime::heap::Allocator = system_server::runtime::heap::Allocator;

core::arch::global_asm!(
    ".section .text.entry,\"ax\"",
    ".global _start",
    "_start:",
    // a0/a1 still point to the Kernel-owned copied startup words. That original
    // stack stays owned by this Task; its small capacity cannot host the full
    // unoptimized supervisor call chain. The Capsule owns this separate RW
    // segment, with an unmapped page below it, until the System Team retires.
    ".option push",
    ".option norelax",
    "la sp, __system_stack_top",
    "la gp, __global_pointer$",
    ".option pop",
    "li tp, 0",
    "tail system_start",
);

/// Kernel a0/a1 describe word arguments, never argc/argv or a legacy Mold entry.
#[unsafe(no_mangle)]
extern "C" fn system_start(pointer: *const usize, count: usize) -> ! {
    if count != kernel_api::boot::args::LEN
        || pointer.is_null()
        || !pointer.addr().is_multiple_of(align_of::<usize>())
    {
        finish(1, "system: startup arguments");
    }
    // SAFETY: this is the sole Kernel entry. Kernel copied these immutable words
    // to the initial Task's guarded stack; no service can invoke this entry.
    let words = unsafe { core::slice::from_raw_parts(pointer, count) };
    let Some(accounts) = (unsafe { system_server::boot::Accounts::from_words(words) }) else {
        finish(1, "system: boot ledger");
    };
    let Some(license) = accounts.permission(Call::Create) else {
        finish(1, "system: creation license");
    };
    let Some(system_server::boot::Resource::Nest(space)) = accounts.resource(Name::Space) else {
        finish(1, "system: Internal Nest");
    };
    // SAFETY: initial Task, before allocations/workers. It stays the supervisor
    // and owns every heap Memo until the enclosing Team exits.
    if unsafe { system_server::runtime::heap::initialize(license, space) }.is_err() {
        finish(1, "system: heap initialization");
    }
    let boot = match unsafe { system_server::boot::take(words) } {
        Ok(boot) => boot,
        Err(error) => finish(error.code(), "system: boot resources"),
    };
    match system_server::run(boot) {
        Ok(()) => finish(0, "system: complete"),
        Err(reason) => finish(reason, "system: supervision failed"),
    }
}
fn finish(reason: usize, note: &str) -> ! {
    let _ = writeln!(Console, "{note} ({reason})");
    if reason != 0 {
        // Early run/boot errors may still own live same-Team workers.
        system_server::runtime::termination::abort(reason as u64, note);
    }
    unit::exit(
        0,
        reason as u64,
        VirtAddr::new(note.as_ptr().addr()),
        note.len(),
        ExitScope::Task,
    )
}
struct Console;
impl Write for Console {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            let used = debug::put(VirtAddr::new(bytes.as_ptr().addr()), bytes.len().min(256))
                .map_err(|_| fmt::Error)?;
            if used == 0 {
                return Err(fmt::Error);
            }
            bytes = &bytes[used..];
        }
        Ok(())
    }
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    // Formatting stays on the initial/worker guarded stack; no heap allocation
    // is needed even when the allocator itself reports a fatal ownership error.
    let _ = writeln!(Console, "system: {info}");
    system_server::runtime::termination::abort(
        kernel_api::model::exit::EXIT_PANIC as u64,
        "system: panic",
    )
}

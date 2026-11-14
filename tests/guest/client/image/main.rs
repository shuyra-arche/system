#![no_std]
#![no_main]
use kernel_api::{
    abi::memory::{Action, Query, Snapshot, Value},
    address::VirtAddr,
    model::{
        cell::CellLevel,
        identity::PendId,
        pend::{CreationRights, Operations, PendInfo},
        unit::ExitScope,
    },
};
#[path = "../../../../server/src/runtime/memory.rs"]
mod memory;
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
    if ![0, 2].contains(&count) || stack & 15 != 0 {
        exit(0, 1);
    }
    // SAFETY: Kernel supplies copied argument words and initialized PT_LOADs.
    unsafe {
        if (count == 2 && (*words != 0xa5 || ![0x5a, 0x5b].contains(&*words.add(1))))
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
    // This ordinary Task has a fresh journal. Production Memory starts at ID 1;
    // an initializer that already consumed that ID would conflict here.
    let mut after = PendId::NONE;
    let license = loop {
        let mut bytes = [0; PendInfo::SIZE];
        if kernel_client::call::pend::collect(after, VirtAddr::new(bytes.as_mut_ptr().addr()), 1)
            .unwrap()
            != 1
        {
            exit(0, 5);
        }
        let info = PendInfo::from_bytes(&bytes).unwrap();
        if info
            .limits
            .operations
            .contains(Operations::None(CreationRights::FLAT))
        {
            break info.token;
        }
        after = info.token;
    };
    let mut memory = memory::Memory::new(|_| exit(0, 6));
    let Value::CreatedFlat { token, flat, .. } = memory
        .mutate(Action::CreateFlat {
            license,
            count: 1,
            level: CellLevel::BASE,
        })
        .unwrap()
    else {
        exit(0, 7)
    };
    let Snapshot::Flat(status) = memory.inspect(Query::Flat { token, flat }) else {
        exit(0, 8)
    };
    memory
        .mutate(Action::DeleteFlat {
            token,
            flat,
            expected: status.version,
        })
        .unwrap();
    kernel_client::call::pend::return_(token).unwrap();
    // SAFETY: the argument block remains this Task's Kernel-owned stack.
    if count == 2 && unsafe { *words.add(1) } == 0x5b {
        let mut after = PendId::NONE;
        loop {
            let mut bytes = [0; PendInfo::SIZE];
            assert_eq!(
                kernel_client::call::pend::collect(
                    after,
                    VirtAddr::new(bytes.as_mut_ptr().addr()),
                    1
                )
                .unwrap(),
                1
            );
            let info = PendInfo::from_bytes(&bytes).unwrap();
            if info.mark == kernel_api::model::identity::Mark::of("system-loader/hold") {
                kernel_client::call::sign::trigger(info.token).unwrap();
                loop {
                    kernel_client::call::room::yield_();
                }
            }
            after = info.token;
        }
    }
    exit(17, 0)
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    exit(0, 4)
}

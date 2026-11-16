//! Entry/ledger/heap contract fixture. This is not the System application.
#![no_std]
extern crate alloc;
pub use system_support::support;
#[path = "../../../src/system/app/bootstrap.rs"]
mod bootstrap;
#[path = "../../../src/system/declaration/mod.rs"]
pub mod declaration;
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
    pub use crate::native_heap as heap;
    pub use crate::native_memory as memory;
    pub(crate) use crate::native_view as view;
    pub use crate::termination;
}
pub mod boot {
    pub use crate::bootstrap::{Boot, Fail, take};
    pub use system_support::boot::{Accounts, Catalog, Resource};
}
#[path = "../../../src/system/identity/book/mod.rs"]
pub mod identity_book;
#[path = "../../../src/system/identity/service/mod.rs"]
pub mod identity_service;
#[path = "../../../src/system/operator/authority.rs"]
pub mod operator_authority;
#[path = "../../../src/system/operator/runtime/mod.rs"]
pub mod operator_runtime;
#[path = "../../../src/system/operator/tree/mod.rs"]
pub mod operator_tree;
#[path = "../../../src/system/operator/watch.rs"]
pub mod operator_watch;
#[path = "../../../src/protocol/mod.rs"]
pub mod protocol;
#[path = "../../../src/system/identity/revision.rs"]
pub mod revision;
#[path = "../../../src/system/app/boot/mod.rs"]
pub mod supervisor_boot;
#[path = "../../../src/system/app/waiting/mod.rs"]
pub mod waiting;
#[path = "../../../src/system/app/worker/mod.rs"]
pub mod worker;
pub mod system {
    pub mod operator {
        pub use crate::{
            operator_authority as authority, operator_runtime as runtime, operator_tree as tree,
            operator_watch as watch,
        };
    }
    pub mod app {
        pub use crate::{waiting, worker};
    }
    pub mod identity {
        pub use crate::{identity_book as book, identity_service as service, revision};
    }
}
mod data;
mod failure;
mod services;
pub fn run(boot: boot::Boot) -> Result<(), usize> {
    if cfg!(feature = "abort") {
        failure::run();
    }
    unsafe extern "C" {
        static __system_stack_bottom: u8;
        static __system_stack_top: u8;
    }
    let pointer: usize;
    // Read the real production entry's current stack, not a fixture stack.
    unsafe { core::arch::asm!("mv {}, sp", out(reg) pointer, options(nomem, nostack)) };
    assert!(pointer > (&raw const __system_stack_bottom).addr() + 64 * 1024);
    assert!(pointer <= (&raw const __system_stack_top).addr());
    assert!(pointer.is_multiple_of(16));
    data::run(&boot.accounts);
    let archive =
        unit_contract::bundle::Archive::parse(boot.accounts.view()).map_err(|_| 10usize)?;
    let declarations = declaration::Catalog::decode(archive).map_err(|_| 11usize)?;
    assert_eq!(declarations.system(), "system");
    assert_eq!(declarations.units().len(), 1);
    assert!(declarations.startup().unwrap().is_empty());
    assert_eq!(
        declarations.find("system").unwrap().loader().image,
        "system-bin"
    );
    let allocation = alloc::vec![0x5au8; 3 * 1024 * 1024];
    assert_eq!(allocation.last(), Some(&0x5a));
    assert!(runtime::heap::mappings() > 0);
    drop(allocation);
    services::run(
        boot.accounts
            .permission(kernel_api::boot::ledger::Call::Create)
            .unwrap(),
        match boot
            .accounts
            .resource(kernel_api::boot::ledger::Name::Space)
            .unwrap()
        {
            boot::Resource::Nest(space) => space,
            _ => panic!("boot Space"),
        },
    );
    system_support::debug::put("PASS system-entry words=4 ledger=13 catalog=2 dtb=1 heap=1\n");
    Ok(())
}

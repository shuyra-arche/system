//! Entry/ledger/heap contract fixture. This is not the System application.
#![no_std]
extern crate alloc;
pub use system_support::support;
#[path = "../../../server/src/system/app/bootstrap.rs"]
mod bootstrap;
#[path = "../../../server/src/system/declaration/mod.rs"]
pub mod declaration;
#[path = "../../../server/src/runtime/heap/mod.rs"]
pub mod native_heap;
#[path = "../../../server/src/runtime/memory.rs"]
pub mod native_memory;
#[path = "../../../server/src/runtime/view.rs"]
pub(crate) mod native_view;
pub mod runtime {
    pub use crate::native_heap as heap;
    pub use crate::native_memory as memory;
    pub(crate) use crate::native_view as view;
}
pub mod boot {
    pub use crate::bootstrap::{Boot, Fail, take};
    pub use system_support::boot::{Accounts, Catalog, Resource};
}
#[path = "../../../server/src/system/identity/book/mod.rs"]
pub mod identity_book;
#[path = "../../../server/src/system/identity/service/mod.rs"]
pub mod identity_service;
#[path = "../../../server/src/system/operator/authority.rs"]
pub mod operator_authority;
#[path = "../../../server/src/system/operator/runtime/mod.rs"]
pub mod operator_runtime;
#[path = "../../../server/src/system/operator/tree/mod.rs"]
pub mod operator_tree;
#[path = "../../../server/src/system/operator/watch.rs"]
pub mod operator_watch;
#[path = "../../../server/src/protocol/mod.rs"]
pub mod protocol;
#[path = "../../../server/src/system/identity/revision.rs"]
pub mod revision;
#[path = "../../../server/src/system/app/boot/mod.rs"]
pub mod supervisor_boot;
#[path = "../../../server/src/system/app/waiting/mod.rs"]
pub mod waiting;
#[path = "../../../server/src/system/app/worker/mod.rs"]
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
mod services;
pub fn run(boot: boot::Boot) -> Result<(), usize> {
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
    system_support::debug::put("PASS system-entry words=4 ledger=11 catalog=2 dtb=1 heap=1\n");
    Ok(())
}

#![no_std]
extern crate alloc;
#[path = "../../server/src/system/operator/authority.rs"]
pub mod authority;
pub use tree::gate;

#[path = "../../server/src/protocol/mod.rs"]
pub mod protocol;
#[path = "../../server/src/system/operator/runtime/mod.rs"]
pub mod runtime;
#[path = "../../server/src/system/operator/tree/mod.rs"]
pub mod tree;
#[path = "../../server/src/system/app/waiting/mod.rs"]
pub mod waiting;
#[path = "../../server/src/system/operator/watch.rs"]
pub mod watch;
pub mod system {
    pub mod app {
        pub use crate::waiting;
    }
    pub mod operator {
        pub use crate::{authority, runtime, tree, watch};
    }
}
#[cfg(test)]
mod namespace;

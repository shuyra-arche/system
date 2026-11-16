extern crate alloc;
#[path = "../../src/system/operator/authority.rs"]
pub mod authority;
pub use tree::gate;

#[path = "../../src/protocol/mod.rs"]
pub mod protocol;
#[path = "../../src/system/operator/runtime/mod.rs"]
pub mod runtime;
#[path = "../../src/system/operator/tree/mod.rs"]
pub mod tree;
#[path = "../../src/system/app/waiting/mod.rs"]
pub mod waiting;
#[path = "../../src/system/operator/watch.rs"]
pub mod watch;
pub mod system {
    pub mod app {
        pub use crate::waiting;
    }
    pub mod operator {
        pub use crate::{authority, runtime, tree, watch};
    }
}
mod namespace;

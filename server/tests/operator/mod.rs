extern crate alloc;
pub mod authority {
    include!("../../src/system/operator/authority.rs");

    mod tests {
        include!("authority.rs");
    }
}
pub use tree::gate;

#[path = "../../src/protocol/mod.rs"]
pub mod protocol;
#[path = "../../src/system/operator/runtime/mod.rs"]
pub mod runtime;
pub mod tree {
    include!("../../src/system/operator/tree/mod.rs");

    mod tests {
        include!("roots.rs");
    }
}
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
mod policy;

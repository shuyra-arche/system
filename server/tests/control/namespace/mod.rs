#![allow(dead_code)]
extern crate alloc;

#[path = "../../../src/system/declaration/mod.rs"]
pub(crate) mod declaration;
#[path = "../../../src/system/app/namespace.rs"]
pub(crate) mod real_layout;

mod effects;
pub(crate) mod services {
    pub(crate) use crate::effects::*;
}
mod system {
    pub(crate) use crate::declaration;
    pub(crate) mod app {
        pub(crate) use crate::real_layout as namespace;
        pub(crate) use crate::services;
    }
}

pub(crate) mod installation {
    include!("../../../src/system/control/instance/installation/namespace.rs");

    mod tests {
        include!("cases.rs");
    }
}

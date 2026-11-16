extern crate alloc;

#[path = "../../src/system/declaration/mod.rs"]
#[allow(dead_code)]
pub(crate) mod declaration;
mod system {
    pub(crate) use crate::declaration;
}
#[path = "../../src/system/control/construction/receipts.rs"]
#[allow(dead_code)]
mod construction_receipts;
#[path = "../../src/system/control/installs.rs"]
mod installs;
#[path = "../../src/system/control/operations.rs"]
mod journal;
#[path = "../../src/system/control/program.rs"]
mod program;

mod construction;
mod installations;
mod programs;

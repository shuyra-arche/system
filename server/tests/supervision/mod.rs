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
mod control;
mod declarations;
mod image;
#[path = "../../src/system/control/operations.rs"]
mod journal;
#[path = "../../src/system/control/program.rs"]
mod program;
mod programs;

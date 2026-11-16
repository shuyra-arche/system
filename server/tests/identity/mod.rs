#![allow(dead_code)]
extern crate alloc;
#[path = "../../src/system/identity/book/mod.rs"]
pub mod book;
#[path = "../../src/system/identity/revision.rs"]
mod revision_owner;

#[path = "../../src/system/identity/service/answer.rs"]
pub mod answer;
pub mod system {
    pub mod identity {
        pub use crate::book;
    }
}
mod dispatch;

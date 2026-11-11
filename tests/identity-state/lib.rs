#![no_std]
#![allow(dead_code)]
extern crate alloc;
#[path = "../../server/src/system/identity/book/mod.rs"]
pub mod book;

#[path = "../../server/src/system/identity/service/answer.rs"]
pub mod answer;
pub mod system {
    pub mod identity {
        pub use crate::book;
    }
}
#[cfg(test)]
mod dispatch;

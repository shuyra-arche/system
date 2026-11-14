#![no_std]

extern crate alloc;

#[doc(hidden)]
pub use alloc::format as __format;

pub mod boot;
pub mod protocol;
pub mod runtime;
pub use system_support::{debug, support};

mod system;

pub use system::app::run;
pub use system::control::E_PROGRAM;

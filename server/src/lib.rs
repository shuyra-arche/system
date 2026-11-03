#![no_std]

extern crate alloc;

#[doc(hidden)]
pub use alloc::format as __format;

pub mod boot;
pub use system_support::{debug, support};

#[cfg(feature = "test-support")]
pub mod test_support;

mod system;

pub use system::app::run;
pub use system::control::unit::start::E_PROGRAM;
pub use unit;

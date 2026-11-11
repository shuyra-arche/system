//! Native Kernel startup ledger and immutable Unit archive.
//! The initial supervisor owns the borrowed resources for the Team lifetime.
pub use system_support::boot::{Accounts, Catalog, Dispatch, Resource};
pub use crate::system::app::bootstrap::{Boot, Fail, take};

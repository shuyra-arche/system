//! Immutable Unit bundle and typed Kernel ABI 9 startup ledger.
//! Capability ownership remains with the initial Task. Control IDs, permission
//! Pends and paired View/Sign resources never share an untyped token accessor.

pub mod accounts;
pub mod catalog;

pub use accounts::{Accounts, Dispatch, Resource};
pub use catalog::Catalog;

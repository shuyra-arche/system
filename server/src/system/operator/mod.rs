//! Operator owns namespace state, scoped authority and native subscriptions.
//! Bootstrap receives explicit recipient-local publications; there is no
//! legacy persistent session handshake or discovery by a scanned Mark.
pub(crate) mod authority;
pub(crate) mod runtime;
pub(crate) mod tree;
mod watch;

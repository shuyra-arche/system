//! Release publication, coalesced notification and consume-before-read ordering.
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};
use erra::Error as KernelError;
use kernel_api::{
    abi::call::sign::{SignFail, SignReply},
    model::{identity::PendId, wait::Wait},
};
use kernel_client::call::sign;

#[derive(Clone)]
pub struct Epoch(Arc<AtomicU64>);
impl Default for Epoch {
    fn default() -> Self {
        Self::new()
    }
}
impl Epoch {
    pub fn new() -> Self {
        Self(Arc::new(AtomicU64::new(0)))
    }
    pub fn current(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
}
#[derive(Debug)]
pub enum Error {
    Exhausted,
    Sign(KernelError<SignFail>),
    InvalidEvent,
}

pub(super) struct Publisher {
    epoch: Epoch,
    pending: bool,
}
impl Publisher {
    pub(super) fn new(epoch: Epoch) -> Self {
        Self {
            epoch,
            pending: false,
        }
    }
    pub(super) fn record(&mut self) -> Result<(), Error> {
        self.epoch
            .0
            .try_update(Ordering::Release, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| Error::Exhausted)?;
        self.pending = true;
        Ok(())
    }
    pub(super) fn flush(&mut self, notify: PendId) -> Result<(), Error> {
        if self.pending {
            match sign::trigger(notify) {
                Ok(()) => (),
                Err(error) if error.source.is_busy() => (),
                Err(error) => return Err(Error::Sign(error)),
            }
            self.pending = false;
        }
        Ok(())
    }
}

/// Poll an owned/borrowed notification endpoint. Consume the exact sequence
/// before reading the epoch so a coalesced concurrent update is not lost.
pub fn consume(token: PendId, epoch: &Epoch) -> Result<u64, Error> {
    match sign::listen(token, Wait::Poll).map_err(Error::Sign)? {
        SignReply::Pending => (),
        SignReply::Triggered { sequence } => match sign::consume(token, sequence) {
            Ok(()) => (),
            Err(error) if error.source.is_busy() => (),
            Err(error) => return Err(Error::Sign(error)),
        },
        _ => return Err(Error::InvalidEvent),
    }
    Ok(epoch.current())
}

#[cfg(test)]
#[path = "../../../../tests/identity-state/revision.rs"]
mod tests;

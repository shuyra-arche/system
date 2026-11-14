use core::fmt;
use erra::Error;
use kernel_api::abi::call::pend::PendFail;
use twine::{
    sign::Watch,
    view::clone::{ReceiveError, Reply, ReplyError},
};

#[derive(Debug)]
pub enum CleanupError {
    Watch(Error<PendFail>),
    Reply(ReplyError),
}

impl CleanupError {
    /// The retained cleanup can make progress after yielding. Other failures
    /// require caller action and must not be silently retried as contention.
    pub fn is_busy(&self) -> bool {
        match self {
            Self::Watch(error) => error.source.is_busy(),
            Self::Reply(ReplyError::Return(error)) => error.source.is_busy(),
            Self::Reply(ReplyError::Receive(ReceiveError::Pull(error))) => error.source.is_busy(),
            Self::Reply(ReplyError::Receive(ReceiveError::Mapping(error))) => error.is_busy(),
            Self::Reply(ReplyError::Receive(ReceiveError::Return(error))) => error.source.is_busy(),
            _ => false,
        }
    }
}

/// Owns every acquired route, including resources from a failed submission.
/// A failed finish retains the failing object and all remaining work for retry.
/// Drop remains best effort. Use `finish` to observe completion.
#[derive(Default)]
#[must_use = "finish the cleanup, retrying on failure"]
pub struct Cleanup {
    pub(super) reply: Option<Reply<crate::Context>>,
    pub(super) incoming: Option<Watch>,
    pub(super) outgoing: Option<Watch>,
}

impl fmt::Debug for Cleanup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cleanup")
            .field("reply", &self.reply.is_some())
            .field("incoming", &self.incoming.is_some())
            .field("outgoing", &self.outgoing.is_some())
            .finish()
    }
}

impl Cleanup {
    pub fn is_finished(&self) -> bool {
        self.reply.is_none() && self.incoming.is_none() && self.outgoing.is_none()
    }

    /// Closing a reply route does not cancel work already submitted to System.
    pub fn finish(&mut self) -> Result<(), CleanupError> {
        if let Some(watch) = self.outgoing.as_mut() {
            watch.finish().map_err(CleanupError::Watch)?;
            self.outgoing = None;
        }
        if let Some(watch) = self.incoming.as_mut() {
            watch.finish().map_err(CleanupError::Watch)?;
            self.incoming = None;
        }
        if let Some(reply) = self.reply.as_mut() {
            reply.finish().map_err(CleanupError::Reply)?;
            self.reply = None;
        }
        Ok(())
    }
}

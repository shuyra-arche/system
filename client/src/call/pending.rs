use alloc::vec::Vec;
use core::marker::PhantomData;
use kernel_api::model::{identity::TaskId, wait::Wait};
use kernel_client::call::room;
use twine::view::clone::{ReceiveError as TwineReceiveError, ReplyError};
use weave::{
    Codec, Contract, Operation, Received,
    transport::{self, ReceiveError},
};

use super::{Cleanup, CleanupError, Error, check_wait};

pub type Response<O> = Received<<<O as Contract>::Response as Codec>::Decoded, TaskId>;

enum State {
    Waiting,
    Mapping,
    Complete,
}

/// One submitted request. No receive error ever resends the request.
#[must_use = "receive or abandon the response, then finish the reply route"]
pub struct Pending<O: Operation> {
    cleanup: Cleanup,
    buffer: Vec<u8>,
    wait: Wait,
    state: State,
    operation: PhantomData<fn() -> O>,
}

impl<O: Operation> Pending<O> {
    pub(super) fn new(cleanup: Cleanup, buffer: Vec<u8>, wait: Wait) -> Self {
        Self {
            cleanup,
            buffer,
            wait,
            state: State::Waiting,
            operation: PhantomData,
        }
    }

    /// Try the response once. A missing frame leaves the call usable. Decode,
    /// sender and response-variant failures are terminal.
    pub fn poll(&mut self) -> Result<Option<Response<O>>, Error> {
        if matches!(self.state, State::Complete) {
            return Err(Error::Inactive);
        }
        let result = transport::receive::<<O::Interface as Contract>::Response, _>(
            self.cleanup.reply.as_mut().ok_or(Error::Inactive)?,
            &mut self.buffer,
        );
        match result {
            Ok(Received { value, metadata }) => {
                self.state = State::Complete;
                let value = O::unwrap_response(value).ok_or(Error::WrongOperation)?;
                Ok(Some(Received { value, metadata }))
            }
            Err(ReceiveError::Transport(ReplyError::Receive(TwineReceiveError::Pull(error))))
                if error.source.is_busy() =>
            {
                self.state = State::Waiting;
                Ok(None)
            }
            Err(ReceiveError::Transport(ReplyError::Receive(TwineReceiveError::Mapping(
                error,
            )))) if error.is_busy() => {
                // The frame has left the queue. Twine retains its Payload.
                // Waiting for queue readability here could deadlock forever.
                self.state = State::Mapping;
                Ok(None)
            }
            Err(error) => {
                self.state = State::Complete;
                Err(Error::Receive(error))
            }
        }
    }

    /// Wait under the original absolute deadline. Readiness is always followed
    /// by a real receive. A deadline never reports early or extends on retries.
    /// `Poll` returns NotReady while preserving the call for a later poll.
    pub fn wait(&mut self) -> Result<Response<O>, Error> {
        loop {
            if let Some(response) = self.poll()? {
                return Ok(response);
            }
            if let Err(error) = check_wait(self.wait) {
                if matches!(error, Error::TimedOut) {
                    self.state = State::Complete;
                }
                return Err(error);
            }
            if matches!(self.state, State::Mapping) {
                room::yield_();
            } else {
                self.cleanup
                    .incoming
                    .as_mut()
                    .ok_or(Error::Inactive)?
                    .listen(self.wait)
                    .map_err(Error::Watch)?;
                // A concurrent reader or reservation can consume readiness.
                room::yield_();
            }
        }
    }

    /// Abandon further receives and release resources. Retry after any failure.
    pub fn finish(&mut self) -> Result<(), CleanupError> {
        self.state = State::Complete;
        self.cleanup.finish()
    }

    pub fn is_finished(&self) -> bool {
        self.cleanup.is_finished()
    }

    /// Abandon future receives and transfer every cleanup obligation to an
    /// owner that can retain and retry it. This does not cancel submitted work.
    pub fn into_cleanup(self) -> Cleanup {
        self.cleanup
    }
}

//! Task-local read-only authority queries with retained cleanup between calls.
use core::cell::RefCell;
use kernel_api::model::{
    identity::{PendId, TaskId},
    wait::Wait,
};
use system_api::{
    identity::{self as api, operations as op},
    operator::Wired,
    value::Outcome,
};
use weave::{Contract, Operation};

use super::Client;
use crate::{Cleanup, CleanupError, Error};

#[derive(Debug)]
pub enum QueryError {
    InvalidWiring,
    Busy,
    Call(Error),
    Cleanup(CleanupError),
    Rejected(api::Fail),
}

/// Borrows three independently authorized query endpoints in this Task's
/// table. It caches their identities, never an authorization snapshot.
/// The owner retains any unfinished per-call cleanup until finish succeeds.
#[must_use = "finish retained query cleanup before leaving its owning Task"]
pub struct TaskQuery {
    authority: TaskId,
    license: PendId,
    context: crate::Context,
    resolve: PendId,
    matches: PendId,
    same: PendId,
    state: RefCell<State>,
}

#[derive(Default)]
struct State {
    cleanup: Option<Cleanup>,
}
impl State {
    fn finish(&mut self) -> Result<(), QueryError> {
        if let Some(cleanup) = self.cleanup.as_mut() {
            cleanup.finish().map_err(QueryError::Cleanup)?;
            self.cleanup = None;
        }
        Ok(())
    }

    fn call<O: Operation<Interface = op::Interface>>(
        &mut self,
        query: &TaskQuery,
        endpoint: PendId,
        request: <O as Contract>::Request,
        wait: Wait,
    ) -> Result<crate::Response<O>, QueryError> {
        self.finish()?;
        let mut client = Client::bind(
            endpoint,
            query.authority,
            query.license,
            query.context.clone(),
        )
        .map_err(QueryError::Call)?;
        let result = match client.begin::<O>(request, wait) {
            Ok(mut pending) => {
                let result = pending.wait();
                self.cleanup = Some(pending.into_cleanup());
                result
            }
            Err(failure) => {
                self.cleanup = Some(failure.cleanup);
                Err(failure.error)
            }
        };
        // No result becomes an authority decision while resource teardown is
        // incomplete. A failed finish leaves its owner available for retry.
        self.finish()?;
        result.map_err(QueryError::Call)
    }
}

impl TaskQuery {
    pub fn from_wired(
        wired: Wired,
        license: PendId,
        context: crate::Context,
    ) -> Result<Self, QueryError> {
        if !wired.valid() {
            return Err(QueryError::InvalidWiring);
        }
        let authority = wired
            .authority
            .try_into()
            .map_err(|_| QueryError::InvalidWiring)?;
        let resolve = wired
            .resolve
            .try_into()
            .map_err(|_| QueryError::InvalidWiring)?;
        let matches = wired
            .matches
            .try_into()
            .map_err(|_| QueryError::InvalidWiring)?;
        let same = wired
            .same
            .try_into()
            .map_err(|_| QueryError::InvalidWiring)?;
        for endpoint in [resolve, matches, same] {
            Client::bind(endpoint, authority, license, context.clone())
                .map_err(QueryError::Call)?;
        }
        Ok(Self {
            authority,
            license,
            context,
            resolve,
            matches,
            same,
            state: RefCell::new(State::default()),
        })
    }

    pub fn resolve(&self, task: api::Task, wait: Wait) -> Result<Option<api::Binding>, QueryError> {
        let response = self
            .state
            .try_borrow_mut()
            .map_err(|_| QueryError::Busy)?
            .call::<op::Resolve>(self, self.resolve, task, wait)?;
        match response.value {
            Outcome::Success(binding) => Ok(binding.0),
            Outcome::Failure(fail) => Err(QueryError::Rejected(fail)),
        }
    }

    pub fn matches(&self, request: api::Matches, wait: Wait) -> Result<api::Match, QueryError> {
        let response = self
            .state
            .try_borrow_mut()
            .map_err(|_| QueryError::Busy)?
            .call::<op::Matches>(self, self.matches, request, wait)?;
        match response.value {
            Outcome::Success(reply) => Ok(reply.value.0),
            Outcome::Failure(fail) => Err(QueryError::Rejected(fail)),
        }
    }

    pub fn same(&self, request: api::Same, wait: Wait) -> Result<api::Match, QueryError> {
        let response = self
            .state
            .try_borrow_mut()
            .map_err(|_| QueryError::Busy)?
            .call::<op::Same>(self, self.same, request, wait)?;
        match response.value {
            Outcome::Success(reply) => Ok(reply.value.0),
            Outcome::Failure(fail) => Err(QueryError::Rejected(fail)),
        }
    }

    pub fn finish(&self) -> Result<(), QueryError> {
        self.state
            .try_borrow_mut()
            .map_err(|_| QueryError::Busy)?
            .finish()
    }
}

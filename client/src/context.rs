//! One Task's shared native-memory context for every retained reply delivery.
use alloc::rc::Rc;
use core::cell::RefCell;
use erra::Error;
use kernel_api::{
    abi::{
        call::pend::PendFail,
        memory::{Call, RequestId},
    },
    model::{identity::PendId, nest::NestId, pend::PendInfo},
};
use kernel_client::memory::{self, Response};
use twine::view::memory::Memory;

/// Shares the caller's actual Nest and request allocator between clients,
/// outstanding calls and cleanup. Cloning never starts a new request sequence.
/// The context and every clone must stay in their original Task.
#[derive(Clone)]
pub struct Context(Rc<RefCell<dyn Memory>>);

impl Context {
    /// Use the same context (or clones) for all System calls in this Task.
    /// The supplied provider must also share its request allocator with any
    /// other native-memory users, including the caller's heap and Twine rings.
    pub fn new(memory: impl Memory + 'static) -> Self {
        Self(Rc::new(RefCell::new(memory)))
    }
}

// SAFETY: RefCell serializes access to the original Memory implementation.
// Rc keeps its request sequence alive through pending responses and cleanup;
// it also prevents transferring this Task-local context between Rust threads.
unsafe impl Memory for Context {
    fn space(&self) -> (PendId, NestId) {
        self.0.borrow().space()
    }
    fn next_request(&mut self) -> Option<RequestId> {
        self.0.borrow_mut().next_request()
    }
    fn invoke(&mut self, call: Call) -> Result<Response, memory::Error> {
        self.0.borrow_mut().invoke(call)
    }
    fn inspect_pend(&mut self, token: PendId) -> Result<PendInfo, Error<PendFail>> {
        self.0.borrow_mut().inspect_pend(token)
    }
    fn return_pend(&mut self, token: PendId) -> Result<(), Error<PendFail>> {
        self.0.borrow_mut().return_pend(token)
    }
}

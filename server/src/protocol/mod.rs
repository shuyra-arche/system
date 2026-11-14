//! Protocol v4 ingress over ABI 11 native deliveries. Route validation precedes all service side effects.
//! Endpoints borrow their queue and own their watch and retained deliveries.

use alloc::vec::Vec;
use core::{cell::Cell, convert::Infallible, fmt, marker::PhantomData};
use erra::Error as KernelError;
use kernel_api::{
    abi::call::{clone_view::CloneFail, pend::PendFail, sign::SignFail},
    model::{
        identity::{PendId, TaskId},
        wait::Wait,
    },
};
use kernel_client::call::room;
use system_protocol as api;
use twine::{
    sign::{Watch, WatchError},
    view::clone::{ReceiveError, Receiver, ReplyError, Sender},
};
use weave::{
    Codec, Contract,
    server::{self, ServeError},
    transport,
};

pub type Control = Endpoint<api::control::operations::Interface, 1>;
pub type Identity = Endpoint<api::identity::operations::Interface, 2>;
pub type Operator = Endpoint<api::operator::operations::Interface, 3>;
pub type Loader = Endpoint<api::loader::operations::Interface, 4>;
pub type Supply = Endpoint<api::supply::operations::Interface, 5>;
pub type Authority = Endpoint<api::operator::authority::operations::Interface, 6>;
pub type Retirement = Endpoint<api::retirement::operations::Interface, 7>;
pub type Setup = Endpoint<api::operator::setup::operations::Interface, 8>;
pub type Export = Endpoint<api::identity::export::operations::Interface, 9>;

#[derive(Debug)]
pub enum RouteError {
    InvalidId,
    Reply(ReplyError),
}

pub type ServeFailure = ServeError<ReceiveError, RouteError, Infallible, KernelError<CloneFail>>;

#[derive(Debug)]
pub enum Error {
    Unbounded,
    Allocation,
    CreateWatch(KernelError<SignFail>),
    Watch(WatchError),
    Serve(ServeFailure),
    NotReady,
    TimedOut,
    Inactive,
}

#[derive(Debug)]
pub enum CleanupError {
    Watch(KernelError<PendFail>),
    Delivery(ReceiveError),
}

/// Does not own the borrowed queue. Finish before its owner withdraws it.
#[derive(Default)]
#[must_use = "finish retained deliveries and the watch, retrying on failure"]
pub struct Cleanup {
    receiver: Option<Receiver<system_client::Context>>,
    watch: Option<Watch>,
}
impl fmt::Debug for Cleanup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cleanup")
            .field("receiver", &self.receiver.is_some())
            .field("watch", &self.watch.is_some())
            .finish()
    }
}
impl Cleanup {
    pub fn finish(&mut self) -> Result<(), CleanupError> {
        if let Some(watch) = self.watch.as_mut() {
            watch.finish().map_err(CleanupError::Watch)?;
            self.watch = None;
        }
        if let Some(receiver) = self.receiver.as_mut() {
            receiver.discard().map_err(CleanupError::Delivery)?;
            self.receiver = None;
        }
        Ok(())
    }
    pub fn is_finished(&self) -> bool {
        self.watch.is_none() && self.receiver.is_none()
    }
}

#[derive(Debug)]
#[must_use = "finish any retained watch after binding fails"]
pub struct BindFailure {
    pub error: Error,
    pub cleanup: Cleanup,
}

struct Call<I, const DOMAIN: u16>(PhantomData<fn() -> I>);
impl<I: Contract, const DOMAIN: u16> Contract for Call<I, DOMAIN> {
    type Request = api::value::Request<I::Request, DOMAIN>;
    type Response = I::Response;
}

/// Serial service ingress. Each decoded request invokes its handler at most once.
/// A send failure can follow side effects. Do not dispatch the request again.
#[must_use = "finish ingress before withdrawing the borrowed queue"]
pub struct Endpoint<I: Contract, const DOMAIN: u16> {
    cleanup: Cleanup,
    request: Vec<u8>,
    response: Vec<u8>,
    active: bool,
    retry_delivery: bool,
    interface: PhantomData<fn() -> I>,
}

impl<I: Contract, const DOMAIN: u16> Endpoint<I, DOMAIN> {
    /// Borrow a queue with PULL and PEEK rights. Create it in the serving Task
    /// so clients can bind its owner as the authenticated reply source.
    #[allow(
        clippy::result_large_err,
        reason = "retain native delivery cleanup without allocating on a failed bind"
    )]
    pub fn bind(
        queue: PendId,
        license: PendId,
        context: system_client::Context,
    ) -> Result<Self, BindFailure> {
        let mut cleanup = Cleanup::default();
        let result = (|| {
            let request = buffer::<<Call<I, DOMAIN> as Contract>::Request>()?;
            let response = buffer::<I::Response>()?;
            let receiver = Receiver::new(queue, context);
            let source = receiver.source();
            cleanup.receiver = Some(receiver);
            cleanup.watch = Some(Watch::create(license).map_err(Error::CreateWatch)?);
            cleanup
                .watch
                .as_mut()
                .unwrap()
                .attach(source, 0)
                .map_err(Error::Watch)?;
            Ok((request, response))
        })();
        match result {
            Ok((request, response)) => Ok(Self {
                cleanup,
                request,
                response,
                active: true,
                retry_delivery: false,
                interface: PhantomData,
            }),
            Err(error) => Err(BindFailure { error, cleanup }),
        }
    }

    /// Receive, validate the exact envelope and reply grant, then dispatch once.
    /// The handler receives the actual Clone sender, never a claimed wire ID.
    /// A malformed frame or invalid grant causes no handler side effects.
    pub fn serve_once(
        &mut self,
        wait: Wait,
        dispatch: impl FnOnce(TaskId, <I::Request as Codec>::Decoded) -> I::Response,
    ) -> Result<(), Error> {
        if !self.active {
            return Err(Error::Inactive);
        }
        let mut dispatch = Some(dispatch);
        loop {
            self.retry_delivery = false;
            let caller = Cell::new(None);
            let result = server::serve_once::<Call<I, DOMAIN>, _, _, _, Infallible>(
                self.cleanup.receiver.as_mut().ok_or(Error::Inactive)?,
                |request, actual| {
                    let token =
                        PendId::try_from(request.reply).map_err(|_| RouteError::InvalidId)?;
                    let sender = Sender::reply(token, actual).map_err(RouteError::Reply)?;
                    caller.set(Some(actual));
                    Ok(sender)
                },
                |request| {
                    Ok(dispatch.take().unwrap()(
                        caller.get().unwrap(),
                        request.body,
                    ))
                },
                &mut self.request,
                &mut self.response,
            );
            match result {
                Ok(()) => return Ok(()),
                Err(ServeError::Receive(transport::ReceiveError::Transport(
                    ReceiveError::Pull(error),
                ))) if error.source.is_busy() => {
                    check_wait(wait)?;
                    self.cleanup
                        .watch
                        .as_mut()
                        .ok_or(Error::Inactive)?
                        .listen(wait)
                        .map_err(Error::Watch)?;
                    room::yield_();
                }
                Err(ServeError::Receive(transport::ReceiveError::Transport(
                    ReceiveError::Mapping(error),
                ))) if error.is_busy() => {
                    self.retry_delivery = true;
                    check_wait(wait)?;
                    room::yield_();
                }
                Err(ServeError::Receive(transport::ReceiveError::Transport(
                    ReceiveError::Return(error),
                ))) if error.source.is_busy() => {
                    self.retry_delivery = true;
                    check_wait(wait)?;
                    room::yield_();
                }
                Err(error) => return Err(Error::Serve(error)),
            }
        }
    }

    /// Stops further dispatch even when cleanup must be retried. The queue
    /// owner must separately return its root to revoke external send grants.
    pub fn finish(&mut self) -> Result<(), CleanupError> {
        self.active = false;
        self.cleanup.finish()
    }

    pub fn is_finished(&self) -> bool {
        self.cleanup.is_finished()
    }

    /// Polling multiplexers must retry a retained payload without waiting for
    /// another queue event. No handler has run for the pending receive.
    pub fn needs_retry(&self) -> bool {
        self.active && self.retry_delivery
    }
}

fn buffer<T: Codec>() -> Result<Vec<u8>, Error> {
    let size = T::SIZE.maximum().ok_or(Error::Unbounded)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| Error::Allocation)?;
    bytes.resize(size, 0);
    Ok(bytes)
}

fn check_wait(wait: Wait) -> Result<(), Error> {
    match wait {
        Wait::Poll => Err(Error::NotReady),
        Wait::At(at) if room::clock() >= at => Err(Error::TimedOut),
        _ => Ok(()),
    }
}

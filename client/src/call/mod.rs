//! System-specific routing and call lifetime. Codec and IPC stay in their providers.

mod cleanup;
mod pending;

pub use cleanup::{Cleanup, CleanupError};
pub use pending::{Pending, Response};

use alloc::vec::Vec;
use core::fmt;
use erra::Error as KernelError;
use kernel_api::{
    abi::call::{clone_view::CloneFail, pend::PendFail, sign::SignFail},
    address::VirtAddr,
    model::{
        identity::{PendId, TaskId},
        modifiable::Modifiable,
        pend::{CloneRights, Operations, PendInfo, PendStatus, Target, ViewTarget},
        wait::Wait,
    },
};
use kernel_client::call::{pend, room};
use twine::{
    sign::{Watch, WatchError},
    view::clone::{Reply, ReplyError, Sender},
};
use weave::{Codec, Contract, Operation, codec, transport::ReceiveError};

#[derive(Debug)]
pub enum Error {
    Inspect(KernelError<PendFail>),
    InvalidEndpoint,
    Unbounded,
    Allocation,
    Encode,
    CreateReply(KernelError<CloneFail>),
    Grant(ReplyError),
    CreateWatch(KernelError<SignFail>),
    Watch(WatchError),
    Send(KernelError<CloneFail>),
    Receive(ReceiveError<ReplyError>),
    WrongOperation,
    NotReady,
    TimedOut,
    Inactive,
}

/// A failed submission can still own resources. Finish its cleanup explicitly.
#[must_use = "finish the retained cleanup, retrying on failure"]
pub struct Failure {
    pub error: Error,
    pub cleanup: Cleanup,
}

impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Failure")
            .field("error", &self.error)
            .field("cleanup", &self.cleanup)
            .finish()
    }
}

pub(crate) struct Connection {
    sender: Sender,
    peer: TaskId,
    license: PendId,
    context: crate::Context,
}

impl Connection {
    pub(crate) fn bind(
        entry: PendId,
        peer: TaskId,
        license: PendId,
        context: crate::Context,
    ) -> Result<Self, Error> {
        let mut bytes = [0; PendInfo::SIZE];
        pend::inspect(entry, VirtAddr::new(bytes.as_mut_ptr().addr())).map_err(Error::Inspect)?;
        let info = PendInfo::from_bytes(&bytes).ok_or(Error::InvalidEndpoint)?;
        validate_endpoint(&info, entry, peer)?;
        Ok(Self {
            sender: Sender::new(entry),
            peer,
            license,
            context,
        })
    }

    #[allow(
        clippy::result_large_err,
        reason = "retain cleanup ownership without allocating on an error path"
    )]
    pub(crate) fn begin<O: Operation, const DOMAIN: u16>(
        &mut self,
        request: O::Request,
        wait: Wait,
    ) -> Result<Pending<O>, Failure> {
        let mut cleanup = Cleanup::default();
        let result = (|| {
            type Envelope<O, const DOMAIN: u16> = system_api::value::Request<
                <<O as Operation>::Interface as Contract>::Request,
                DOMAIN,
            >;
            let mut outgoing = buffer::<Envelope<O, DOMAIN>>()?;
            let incoming = buffer::<<O::Interface as Contract>::Response>()?;
            cleanup.reply = Some(
                Reply::create(self.license, incoming.len(), self.context.clone())
                    .map_err(Error::CreateReply)?,
            );
            let reply = cleanup.reply.as_mut().unwrap();
            let route = loop {
                match reply.grant(self.peer) {
                    Ok(route) => break route,
                    // ABI 9 Accord Busy is before registration.publish. Twine
                    // leaves Reply Local, so retry the same root and recipient.
                    Err(ReplyError::Accord(error)) if error.source.is_busy() => {
                        check_wait(wait)?;
                        room::yield_();
                        check_wait(wait)?;
                    }
                    Err(error) => return Err(Error::Grant(error)),
                }
            };
            let source = reply.source();
            cleanup.incoming = Some(Watch::create(self.license).map_err(Error::CreateWatch)?);
            cleanup
                .incoming
                .as_mut()
                .unwrap()
                .attach(source, 0)
                .map_err(Error::Watch)?;
            let envelope = Envelope::<O, DOMAIN> {
                reply: route.into(),
                body: O::wrap_request(request),
            };
            let used = codec::encode(&envelope, &mut outgoing).ok_or(Error::Encode)?;
            let mut attempted = false;
            loop {
                if attempted {
                    check_wait(wait)?;
                }
                attempted = true;
                match self.sender.push(&outgoing[..used]) {
                    Ok(()) => return Ok(incoming),
                    // Clone Busy leaves no submitted frame. Other failures must
                    // not be retried. Success leaves this loop permanently.
                    Err(error) if error.source.is_busy() => {
                        check_wait(wait)?;
                        if cleanup.outgoing.is_none() {
                            cleanup.outgoing =
                                Some(Watch::create(self.license).map_err(Error::CreateWatch)?);
                            cleanup
                                .outgoing
                                .as_mut()
                                .unwrap()
                                .attach(self.sender.source(), 0)
                                .map_err(Error::Watch)?;
                        }
                        cleanup
                            .outgoing
                            .as_mut()
                            .unwrap()
                            .listen(wait)
                            .map_err(Error::Watch)?;
                        check_wait(wait)?;
                        // Push readiness promises some space, not enough for this
                        // frame. Yield before rechecking to let the consumer run.
                        room::yield_();
                    }
                    Err(error) => return Err(Error::Send(error)),
                }
            }
        })();
        match result {
            Ok(buffer) => Ok(Pending::new(cleanup, buffer, wait)),
            Err(error) => Err(Failure { error, cleanup }),
        }
    }
}

fn validate_endpoint(info: &PendInfo, entry: PendId, peer: TaskId) -> Result<(), Error> {
    let Operations::Clone(rights) = info.limits.operations else {
        return Err(Error::InvalidEndpoint);
    };
    if peer.get() == 0
        || info.token != entry
        || info.owner != peer
        || info.status != PendStatus::Active
        || !matches!(info.target, Target::View(ViewTarget::Clone(_)))
        || !rights.contains(CloneRights::PUSH | CloneRights::PEEK)
        || info.limits.modifiable != Modifiable::Yes
    {
        return Err(Error::InvalidEndpoint);
    }
    Ok(())
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

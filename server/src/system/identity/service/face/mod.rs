use erra::Error as KernelError;
use kernel_api::{
    abi::call::{clone_view::CloneFail, pend::PendFail},
    address::VirtAddr,
    model::{
        clone_view::CloneLimits,
        identity::{Mark, PendId, TaskId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, CloneRights, Distributable, Limits, Operations, PendRange,
        },
    },
};
use kernel_client::call::{clone_view, pend};
use system_protocol::identity::Grant;
use weave::{Codec, Contract};

#[derive(Clone, Copy, Debug)]
pub struct Publication {
    pub grant: Grant,
    /// Already granted in the controller's table, with narrowed Push/Peek.
    pub token: system_protocol::PendId,
}

#[derive(Debug)]
pub enum Error {
    Queue(KernelError<CloneFail>),
    Bind(crate::protocol::Error),
    Grant(KernelError<PendFail>),
}
#[derive(Debug)]
pub enum CleanupError {
    Ingress(crate::protocol::CleanupError),
    Return(KernelError<PendFail>),
}

pub(super) struct Face {
    pub(super) grant: Grant,
    pub(super) root: Option<PendId>,
    pub(super) endpoint: Option<crate::protocol::Identity>,
    partial: Option<crate::protocol::Cleanup>,
    publication: Option<Publication>,
}
impl Face {
    pub(super) fn new(grant: Grant) -> Self {
        Self {
            grant,
            root: None,
            endpoint: None,
            partial: None,
            publication: None,
        }
    }
    pub(super) fn initialize(&mut self, license: PendId, controller: TaskId) -> Result<(), Error> {
        type Request = <system_protocol::identity::Call as Contract>::Request;
        let max_len = Request::SIZE.maximum().expect("bounded Identity protocol");
        let root = clone_view::create(
            license,
            Mark::NONE,
            CloneLimits {
                max_len,
                max_messages: 16,
                max_bytes: max_len * 16,
            },
        )
        .map_err(Error::Queue)?;
        self.root = Some(root);
        self.endpoint = Some(match crate::protocol::Identity::bind(root, license) {
            Ok(endpoint) => endpoint,
            Err(failure) => {
                self.partial = Some(failure.cleanup);
                return Err(Error::Bind(failure.error));
            }
        });
        let request = AccordRequest {
            src: root,
            target: AccordTarget::Task(controller),
            mark: Mark::NONE,
            limits: Limits {
                operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
                modifiable: Modifiable::Yes,
                distributable: Distributable::Yes,
                range: PendRange::Inherit,
            },
        };
        let token = pend::accord(VirtAddr::new(request.to_bytes().as_ptr().addr()))
            .map_err(Error::Grant)?;
        self.publication = Some(Publication {
            grant: self.grant,
            token: token.into(),
        });
        Ok(())
    }
    pub(super) fn publication(&self) -> Option<&Publication> {
        self.publication.as_ref()
    }
    pub(super) fn finish(&mut self) -> Result<(), CleanupError> {
        if let Some(cleanup) = self.partial.as_mut() {
            cleanup.finish().map_err(CleanupError::Ingress)?;
            self.partial = None;
        }
        if let Some(endpoint) = self.endpoint.as_mut() {
            endpoint.finish().map_err(CleanupError::Ingress)?;
            self.endpoint = None;
        }
        if let Some(root) = self.root {
            pend::return_(root).map_err(CleanupError::Return)?;
            self.root = None;
        }
        self.publication = None;
        Ok(())
    }
}

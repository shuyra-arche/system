//! One service-owned ingress and its retained construction/cleanup progress.
use crate::system::operator::authority::{GrantKey, GrantRef};
use kernel_api::{
    abi::call::{clone_view::CloneFail, pend::PendFail},
    address::VirtAddr,
    model::{
        clone_view::CloneLimits,
        identity::{Mark, PendId, TaskId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, CloneRights, Distributable, Limits, Operations, PendScope,
        },
    },
};
use kernel_client::call::{clone_view, pend};
use weave::{Codec, Contract};
#[derive(Clone, Copy)]
pub(super) enum Role {
    Discovery,
    Root,
    Setup,
    Grant {
        key: GrantKey,
        reference: GrantRef,
        holder: TaskId,
    },
}
pub(super) enum Ingress {
    Discovery(crate::protocol::Operator),
    Authority(crate::protocol::Authority),
    Setup(crate::protocol::Setup),
}
#[derive(Debug)]
pub enum Error {
    Queue(erra::Error<CloneFail>),
    Bind(crate::protocol::Error),
    Grant(erra::Error<PendFail>),
    Cleanup(crate::protocol::CleanupError),
    Return(erra::Error<PendFail>),
}
pub(super) struct Face {
    pub role: Role,
    pub root: Option<PendId>,
    pub ingress: Option<Ingress>,
    partial: Option<crate::protocol::Cleanup>,
    pub remote: Option<system_protocol::PendId>,
    pub attached: bool,
    pub closing: bool,
}
impl Face {
    pub fn new(role: Role) -> Self {
        Self {
            role,
            root: None,
            ingress: None,
            partial: None,
            remote: None,
            attached: false,
            closing: false,
        }
    }
    pub fn initialize(
        &mut self,
        license: PendId,
        context: system_client::Context,
    ) -> Result<(), Error> {
        if self.root.is_none() {
            let max_len = match self.role {
                Role::Discovery => <<system_protocol::operator::Call as Contract>::Request as Codec>::SIZE.maximum(),
                Role::Setup => <<system_protocol::operator::setup::Call as Contract>::Request as Codec>::SIZE.maximum(),
                _ => <<system_protocol::operator::authority::Call as Contract>::Request as Codec>::SIZE.maximum(),
            }.expect("bounded Operator protocol");
            self.root = Some(
                clone_view::create(
                    license,
                    Mark::NONE,
                    CloneLimits {
                        max_len,
                        max_messages: 16,
                        max_bytes: max_len * 16,
                    },
                )
                .map_err(Error::Queue)?,
            );
        }
        if self.ingress.is_none() {
            if let Some(partial) = self.partial.as_mut() {
                partial.finish().map_err(Error::Cleanup)?;
                self.partial = None;
            }
            let root = self.root.unwrap();
            macro_rules! bind {
                ($kind:ident, $endpoint:path) => {
                    match <$endpoint>::bind(root, license, context) {
                        Ok(endpoint) => Ingress::$kind(endpoint),
                        Err(failure) => {
                            self.partial = Some(failure.cleanup);
                            return Err(Error::Bind(failure.error));
                        }
                    }
                };
            }
            self.ingress = Some(match self.role {
                Role::Discovery => bind!(Discovery, crate::protocol::Operator),
                Role::Setup => bind!(Setup, crate::protocol::Setup),
                _ => bind!(Authority, crate::protocol::Authority),
            });
        }
        Ok(())
    }
    pub fn deliver(&mut self, holder: TaskId, distributable: Distributable) -> Result<(), Error> {
        if self.remote.is_some() {
            return Ok(());
        }
        let request = AccordRequest {
            src: self.root.unwrap(),
            target: AccordTarget::Task(holder),
            mark: Mark::NONE,
            limits: Limits {
                operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
                modifiable: Modifiable::Yes,
                distributable,
                scope: PendScope::Object,
            },
        };
        self.remote = Some(
            pend::accord(VirtAddr::new(request.to_bytes().as_ptr().addr()))
                .map_err(Error::Grant)?
                .into(),
        );
        Ok(())
    }
    pub fn close_ingress(&mut self) -> Result<(), Error> {
        self.closing = true;
        if let Some(partial) = self.partial.as_mut() {
            partial.finish().map_err(Error::Cleanup)?;
            self.partial = None;
        }
        if let Some(ingress) = self.ingress.as_mut() {
            match ingress {
                Ingress::Discovery(endpoint) => endpoint.finish(),
                Ingress::Authority(endpoint) => endpoint.finish(),
                Ingress::Setup(endpoint) => endpoint.finish(),
            }
            .map_err(Error::Cleanup)?;
            self.ingress = None;
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<(), Error> {
        self.close_ingress()?;
        if let Some(root) = self.root {
            pend::return_(root).map_err(Error::Return)?;
            self.root = None;
        }
        self.remote = None;
        Ok(())
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Queue(error) => write!(f, "operator queue: {error:?}"),
            Self::Bind(error) => write!(f, "operator binding: {error:?}"),
            Self::Grant(error) => write!(f, "operator grant: {error:?}"),
            Self::Cleanup(error) => write!(f, "operator cleanup: {error:?}"),
            Self::Return(error) => write!(f, "operator return: {error:?}"),
        }
    }
}

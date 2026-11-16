//! One accepted post-Prepared installation owns all native partial work.
use crate::system::supply::native::{Resource, Source};
use alloc::{string::String, sync::Arc};
use kernel_api::model::identity::{PendId, TaskId};
use system_protocol::{
    supply::{self as api, Fail, Install, InstallState, InstallStatus, LeaseId, State},
    value::Optional,
};

enum Phase {
    Registering,
    Acquiring,
    Granted,
    Releasing,
    Released,
}
#[must_use = "retain the Instance installation until native roots settle"]
pub(crate) struct Delivery {
    request: Arc<Install>,
    executor: TaskId,
    target: TaskId,
    origin: PendId,
    key: String,
    resource: String,
    source: Source,
    phase: Phase,
    withdrawn: bool,
    failure: Option<Fail>,
}
impl Delivery {
    /// Allocation only. The Instance publishes this owner before any Accord.
    pub fn new(
        request: Arc<Install>,
        executor: TaskId,
        target: TaskId,
        origin: PendId,
        key: &str,
        resource: &str,
    ) -> Result<Self, Fail> {
        Ok(Self {
            request,
            executor,
            target,
            origin,
            key: String::from(key),
            resource: String::from(resource),
            source: Source::new(key)?,
            phase: Phase::Registering,
            withdrawn: false,
            failure: None,
        })
    }
    pub fn id(&self) -> api::InstallId {
        self.request.id
    }
    pub fn slot(&self) -> system_protocol::Mark {
        self.request.slot
    }
    pub fn released(&self) -> bool {
        matches!(self.phase, Phase::Released)
    }
    pub fn ready(&self) -> bool {
        matches!(self.phase, Phase::Granted) || (self.withdrawn && self.released())
    }
    pub fn required_failure(&self) -> Option<Fail> {
        if self.withdrawn { None } else { self.failure }
    }
    pub fn cancel(&mut self) {
        self.withdrawn = true;
        if !self.released() {
            self.phase = Phase::Releasing;
        }
    }
    fn fail(&mut self, failure: Fail) {
        self.failure = Some(failure);
        self.phase = Phase::Releasing;
    }
    pub fn advance(&mut self) -> Result<(), Fail> {
        match self.phase {
            Phase::Registering => match self.source.register(Resource {
                name: &self.resource,
                source: self.origin,
                maximum: self.request.rights.0,
                caller: self.executor,
                target: Some(self.target),
                description: &[],
            }) {
                Ok(()) => {
                    let request = api::Acquire {
                        lease: LeaseId::new(1).unwrap(),
                        target: self.target.into(),
                        binding: api::Binding {
                            source: api::Key {
                                name: self.key.clone(),
                            },
                            resource: api::Key {
                                name: self.resource.clone(),
                            },
                            slot: self.request.slot,
                            rights: self.request.rights,
                        },
                    };
                    match self.source.acquire(self.executor, request) {
                        Ok(_) => self.phase = Phase::Acquiring,
                        Err(failure) => self.fail(failure),
                    }
                }
                Err(Fail::Pending) => (),
                Err(failure) => self.fail(failure),
            },
            Phase::Acquiring | Phase::Granted => {
                self.source.advance()?;
                let (state, failure, _) = self
                    .source
                    .evidence(self.executor, LeaseId::new(1).unwrap())
                    .ok_or(Fail::Unknown)?;
                if let Some(failure) = failure {
                    self.fail(failure);
                } else if state == State::Granted {
                    self.phase = Phase::Granted;
                }
            }
            Phase::Releasing => {
                if self.source.finish()? {
                    self.phase = Phase::Released;
                }
            }
            Phase::Released => (),
        }
        Ok(())
    }
    pub fn status(&self) -> InstallStatus {
        let state = match self.phase {
            Phase::Registering | Phase::Acquiring => InstallState::Pending,
            Phase::Granted => InstallState::Granted,
            Phase::Releasing => InstallState::Releasing,
            Phase::Released => InstallState::Released,
        };
        let delivery = if state == InstallState::Granted {
            self.source
                .evidence(self.executor, LeaseId::new(1).unwrap())
                .and_then(|(_, _, delivery)| delivery)
        } else {
            None
        };
        InstallStatus {
            id: self.request.id,
            state,
            instance: Optional(Some(self.request.instance)),
            target: self.target.into(),
            slot: self.request.slot,
            delivery: Optional(delivery),
            failure: Optional(self.failure.map(weave::Coded)),
        }
    }
}

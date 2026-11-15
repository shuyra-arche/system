//! Native resource sources. A provider owns its roots; System does not infer
//! IOMMU, PCI, IRQ or retirement authority from a device description.
pub(crate) mod native;
mod providers;
use alloc::{string::String, vec::Vec};
use kernel_api::model::{
    identity::{Mark, PendId, TaskId},
    pend::Limits,
};
use native::{Resource, Source};
use system_protocol::supply::{self as api, Fail, LeaseId, State};

struct Definition {
    source: String,
    resource: String,
    root: PendId,
    maximum: Limits,
}
#[derive(Default)]
pub(crate) struct Registry {
    definitions: Vec<Definition>,
    providers: providers::Providers,
}
impl Registry {
    pub fn register(&mut self, source: &str, resource: &str, root: PendId, maximum: Limits) {
        self.definitions.push(Definition {
            source: String::from(source),
            resource: String::from(resource),
            root,
            maximum,
        });
    }

    fn named(&self, source: &str, resource: &str) -> Option<(PendId, Limits)> {
        self.definitions
            .iter()
            .find(|row| row.source == source && row.resource == resource)
            .map(|row| (row.root, row.maximum))
            .or_else(|| self.providers.find(source, resource))
    }
    pub fn advance(&mut self, mut exited: impl FnMut(TaskId) -> bool) -> Result<(), Fail> {
        self.providers.advance(
            |caller, root| {
                exited(caller)
                    || match native::inspect(root) {
                        Ok(info) => info.status != kernel_api::model::pend::PendStatus::Active,
                        Err(Fail::Pending) => false,
                        Err(_) => true,
                    }
            },
            native::retire,
        )
    }
    pub fn finish(&mut self) -> Result<bool, Fail> {
        self.providers.close();
        self.advance(|_| false)?;
        Ok(self.providers.closed())
    }
    pub fn marked(&self, mark: Mark, requested: Limits) -> Option<PendId> {
        self.definitions
            .iter()
            .find(|row| {
                crate::system::app::services::inspect(row.root).mark == mark
                    && row.maximum.permits_narrowing(requested)
            })
            .map(|row| row.root)
    }
    pub fn lease(
        &self,
        owner: &mut Leases,
        caller: TaskId,
        target: TaskId,
        bindings: &[api::Binding],
    ) -> Result<(), Fail> {
        owner
            .sources
            .try_reserve_exact(bindings.len())
            .map_err(|_| Fail::Full)?;
        for binding in bindings {
            let (root, maximum) = self
                .named(&binding.source.name, &binding.resource.name)
                .ok_or(Fail::Unknown)?;
            if !maximum.permits_narrowing(binding.rights.0) {
                return Err(Fail::Denied);
            }
            let source = Source::new(&binding.source.name)?;
            // Publish the cleanup owner before registration can derive a root.
            owner.sources.push(source);
            let source = owner.sources.last_mut().unwrap();
            source.register(Resource {
                name: &binding.resource.name,
                source: root,
                maximum: binding.rights.0,
                caller,
                target: Some(target),
                description: &[],
            })?;
            source.acquire(
                caller,
                api::Acquire {
                    lease: LeaseId::new(1).unwrap(),
                    target: target.into(),
                    binding: binding.clone(),
                },
            )?;
        }
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct Leases {
    sources: Vec<Source>,
}
impl Leases {
    pub fn advance(&mut self, caller: TaskId) -> Result<bool, Fail> {
        let mut ready = true;
        for source in &mut self.sources {
            source.advance()?;
            ready &= source.inspect(caller, LeaseId::new(1).unwrap())?.state == State::Granted;
        }
        Ok(ready)
    }
    pub fn finish(&mut self) -> Result<bool, Fail> {
        if let Some(source) = self.sources.last_mut() {
            if source.finish()? {
                self.sources.pop();
            }
            return Ok(false);
        }
        Ok(true)
    }
}

struct ClientSource {
    caller: TaskId,
    key: String,
    resource: String,
    origin: PendId,
    maximum: Limits,
    source: Source,
}
struct Route {
    caller: TaskId,
    lease: LeaseId,
    source: Option<usize>,
}
#[derive(Default)]
pub(crate) struct Service {
    clients: Vec<ClientSource>,
    routes: Vec<Route>,
    closing: bool,
}
impl Service {
    pub fn answer(
        &mut self,
        registry: &mut Registry,
        caller: TaskId,
        provider: Option<&str>,
        policy: &[api::Binding],
        request: api::operations::DecodedRequest,
    ) -> api::operations::Response {
        use api::operations::{DecodedRequest as Q, Response as R};
        macro_rules! answer {
            ($variant:ident, $value:expr) => {
                R::$variant(match $value {
                    Ok(value) => system_protocol::value::Outcome::Success(value),
                    Err(error) => system_protocol::value::Outcome::Failure(error),
                })
            };
        }
        match request {
            Q::Acquire(request) => {
                let value = (|| {
                    if self.closing || request.target.get() != caller.get() as u64 {
                        return Err(Fail::Denied);
                    }
                    let allowed = policy.iter().find(|binding| {
                        binding.source == request.binding.source
                            && binding.resource == request.binding.resource
                            && binding.slot == request.binding.slot
                            && binding.rights.0.permits_narrowing(request.binding.rights.0)
                    });
                    let allowed = allowed.ok_or(Fail::Denied)?;
                    let at = if let Some(route) = self
                        .routes
                        .iter()
                        .find(|row| row.caller == caller && row.lease == request.lease)
                    {
                        let Some(at) = route.source else {
                            return Ok(api::Reply {
                                state: State::Closed,
                                delivery: system_protocol::value::Optional(None),
                            });
                        };
                        if self.clients[at].key != request.binding.source.name {
                            return Err(Fail::Conflict);
                        }
                        at
                    } else {
                        if self.routes.len() == api::MAX_LEASES {
                            return Err(Fail::Full);
                        }
                        self.routes.try_reserve(1).map_err(|_| Fail::Full)?;
                        let (root, maximum) = registry
                            .named(&request.binding.source.name, &request.binding.resource.name)
                            .ok_or(Fail::Unknown)?;
                        if !maximum.permits_narrowing(allowed.rights.0) {
                            return Err(Fail::Denied);
                        }
                        let maximum = allowed.rights.0;
                        let at = if let Some(at) = self.clients.iter().position(|source| {
                            source.caller == caller
                                && source.key == request.binding.source.name
                                && source.resource == request.binding.resource.name
                                && source.origin == root
                                && source.maximum == maximum
                        }) {
                            at
                        } else {
                            // Each cache entry is committed with a new route;
                            // the existing MAX_LEASES route budget bounds both.
                            self.clients.try_reserve(1).map_err(|_| Fail::Full)?;
                            let mut source = Source::new(&request.binding.source.name)?;
                            source.register(Resource {
                                name: &request.binding.resource.name,
                                source: root,
                                maximum,
                                caller,
                                target: None,
                                description: &[],
                            })?;
                            let at = self.clients.len();
                            self.clients.push(ClientSource {
                                caller,
                                key: request.binding.source.name.clone(),
                                resource: request.binding.resource.name.clone(),
                                origin: root,
                                maximum,
                                source,
                            });
                            at
                        };
                        self.routes.push(Route {
                            caller,
                            lease: request.lease,
                            source: Some(at),
                        });
                        at
                    };
                    self.clients[at].source.acquire(caller, request)
                })();
                answer!(Acquire, value)
            }
            Q::Register(request) => {
                let value = if self.closing {
                    Err(Fail::Denied)
                } else if registry
                    .definitions
                    .iter()
                    .any(|row| row.source == request.source.name)
                {
                    Err(Fail::Conflict)
                } else {
                    registry
                        .providers
                        .register(caller, provider, request, native::inspect)
                };
                answer!(Register, value)
            }
            Q::InspectSource(request) => answer!(
                InspectSource,
                registry.providers.inspect(caller, request.id)
            ),
            Q::Withdraw(request) => {
                answer!(Withdraw, registry.providers.withdraw(caller, request.id))
            }
            Q::Inspect(request) => answer!(
                Inspect,
                self.find(caller, request.lease)
                    .and_then(|at| self.clients[at].source.inspect(caller, request.lease))
            ),
            Q::Release(request) => answer!(
                Release,
                self.find(caller, request.lease)
                    .and_then(|at| self.clients[at].source.release(caller, request.lease))
            ),
            Q::Cancel(request) => {
                let value = match self
                    .routes
                    .iter()
                    .position(|row| row.caller == caller && row.lease == request.lease)
                {
                    Some(at) => match self.routes[at].source {
                        Some(source) => self.clients[source].source.cancel(caller, request.lease),
                        None => Ok(api::Reply {
                            state: State::Closed,
                            delivery: system_protocol::value::Optional(None),
                        }),
                    },
                    None if self.routes.len() < api::MAX_LEASES => {
                        if self.routes.try_reserve(1).is_err() {
                            return answer!(Cancel, Err(Fail::Full));
                        }
                        self.routes.push(Route {
                            caller,
                            lease: request.lease,
                            source: None,
                        });
                        Ok(api::Reply {
                            state: State::Closed,
                            delivery: system_protocol::value::Optional(None),
                        })
                    }
                    None => Err(Fail::Full),
                };
                answer!(Cancel, value)
            }
        }
    }
    fn find(&self, caller: TaskId, lease: LeaseId) -> Result<usize, Fail> {
        self.routes
            .iter()
            .find(|row| row.caller == caller && row.lease == lease)
            .and_then(|row| row.source)
            .ok_or(Fail::Unknown)
    }
    pub fn advance(&mut self) -> Result<(), Fail> {
        for source in &mut self.clients {
            if crate::system::app::services::lookup(source.caller)
                .is_none_or(|info| info.state == kernel_api::model::unit::UnitState::Exited)
            {
                source.source.finish()?;
            } else {
                source.source.advance()?;
            }
        }
        Ok(())
    }
    pub fn finish(&mut self) -> Result<bool, Fail> {
        self.closing = true;
        if let Some(source) = self.clients.last_mut() {
            if source.source.finish()? {
                self.clients.pop();
            }
            return Ok(false);
        }
        Ok(true)
    }
}

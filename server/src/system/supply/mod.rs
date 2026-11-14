//! Native resource sources. A provider owns its roots; System does not infer
//! IOMMU, PCI, IRQ or retirement authority from a device description.
pub(crate) mod native;
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
            let row = self
                .definitions
                .iter()
                .find(|row| {
                    row.source == binding.source.name && row.resource == binding.resource.name
                })
                .ok_or(Fail::Unknown)?;
            if !row.maximum.permits_narrowing(binding.rights.0) {
                return Err(Fail::Denied);
            }
            let source = Source::new(&binding.source.name)?;
            // Publish the cleanup owner before registration can derive a root.
            owner.sources.push(source);
            let source = owner.sources.last_mut().unwrap();
            source.register(Resource {
                name: &binding.resource.name,
                source: row.root,
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
        registry: &Registry,
        caller: TaskId,
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
                    let allowed = policy.iter().any(|binding| {
                        binding.source == request.binding.source
                            && binding.resource == request.binding.resource
                            && binding.slot == request.binding.slot
                            && binding.rights.0.permits_narrowing(request.binding.rights.0)
                    });
                    if !allowed {
                        return Err(Fail::Denied);
                    }
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
                        let at = if let Some(at) = self.clients.iter().position(|source| {
                            source.caller == caller && source.key == request.binding.source.name
                        }) {
                            at
                        } else {
                            if self.clients.len() == 64 {
                                return Err(Fail::Full);
                            }
                            self.clients.try_reserve(1).map_err(|_| Fail::Full)?;
                            let at = self.clients.len();
                            self.clients.push(ClientSource {
                                caller,
                                key: request.binding.source.name.clone(),
                                source: Source::new(&request.binding.source.name)?,
                            });
                            for binding in policy
                                .iter()
                                .filter(|binding| binding.source == request.binding.source)
                            {
                                if let Some(definition) = registry.definitions.iter().find(|row| {
                                    row.source == binding.source.name
                                        && row.resource == binding.resource.name
                                }) {
                                    if !definition.maximum.permits_narrowing(binding.rights.0) {
                                        return Err(Fail::Denied);
                                    }
                                    self.clients[at].source.register(Resource {
                                        name: &binding.resource.name,
                                        source: definition.root,
                                        maximum: binding.rights.0,
                                        caller,
                                        target: None,
                                        description: &[],
                                    })?;
                                }
                            }
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

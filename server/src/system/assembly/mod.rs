//! Instance installation owns identity, publication, leases and readiness roots.
//! Control owns execution; Operator and Identity remain the live authorities.
mod retirement;
use crate::system::{
    app::{
        namespace::{Installation as Namespaces, Layout},
        services::{Services, give, release},
    },
    control::identity::{Installed, Roster},
    declaration::Unit,
    supply::{Leases, Registry},
};
use alloc::sync::Arc;
use kernel_api::model::{
    identity::{Mark, PendId, TaskId},
    modifiable::Modifiable,
    pend::{CloneRights, Distributable, Limits, Operations, PendScope},
    sign::SignRights,
    wait::Wait,
};
use kernel_client::call::sign;
pub(crate) use retirement::Gate;
use system_protocol::identity as id;

pub(crate) struct Plan<'a> {
    pub instance: u64,
    pub task: TaskId,
    pub unit: Option<&'a Arc<Unit>>,
    pub identity: id::Install,
}
#[derive(Default)]
pub(crate) struct Installation {
    identity: Option<Installed>,
    pub namespaces: Namespaces,
    supplies: Leases,
    ready: Option<PendId>,
    acknowledged: bool,
    installed: bool,
}
impl Installation {
    pub fn install(
        &mut self,
        services: &Services,
        roster: &mut Roster,
        layout: &mut Layout,
        sources: &Registry,
        plan: Plan<'_>,
    ) -> Result<(), &'static str> {
        let Plan {
            instance,
            task,
            unit,
            identity,
        } = plan;
        self.identity = Some(
            roster
                .install(services, task, identity)
                .map_err(|_| "identity installation")?,
        );
        let grants = unit.map_or(&[][..], |unit| unit.identity().grants.as_slice());
        roster
            .grant(services, task, grants)
            .map_err(|_| "identity grant delivery")?;
        give(
            services.discovery(),
            task,
            Mark::of("system/operator/discovery"),
            Limits {
                operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
                modifiable: Modifiable::Yes,
                distributable: Distributable::No,
                scope: PendScope::Object,
            },
        )
        .map_err(|_| "discovery grant delivery")?;
        if let Some(unit) = unit {
            if let unit_contract::document::Ready::Signal(mark) = unit.control().ready {
                self.ready =
                    Some(sign::create(services.license).map_err(|_| "readiness creation")?);
                give(
                    self.ready.unwrap(),
                    task,
                    Mark::new(mark),
                    Limits {
                        operations: Operations::Sign(SignRights::TRIGGER),
                        modifiable: Modifiable::No,
                        distributable: Distributable::No,
                        scope: PendScope::Object,
                    },
                )
                .map_err(|_| "readiness delivery")?;
            }
            sources
                .lease(&mut self.supplies, services.caller, task, unit.supply())
                .map_err(|_| "declared resource lease")?;
            self.namespaces
                .install(layout, services, instance, task, unit, |mark, limits| {
                    sources.marked(mark, limits)
                })
                .map_err(|_| "namespace installation")?;
        }
        self.installed = true;
        Ok(())
    }
    pub fn prepared(&mut self, services: &Services) -> Result<bool, &'static str> {
        if !self.installed {
            return Ok(false);
        }
        self.supplies
            .advance(services.caller)
            .map_err(|_| "resource delivery")
    }
    pub fn ready(&mut self) -> Result<bool, &'static str> {
        let Some(signal) = self.ready else {
            return Ok(true);
        };
        if self.acknowledged {
            return Ok(true);
        }
        match sign::listen(signal, Wait::Poll) {
            Ok(kernel_api::abi::call::sign::SignReply::Triggered { sequence }) => {
                sign::consume(signal, sequence).map_err(|_| "readiness consumption")?;
                self.acknowledged = true;
                Ok(true)
            }
            Ok(kernel_api::abi::call::sign::SignReply::Pending) => Ok(false),
            Err(error) if error.source.is_busy() => Ok(false),
            _ => Err("readiness source lost"),
        }
    }
    pub fn finish(
        &mut self,
        layout: &mut Layout,
        services: &Services,
    ) -> Result<bool, &'static str> {
        if !self
            .namespaces
            .finish(layout, services)
            .map_err(|_| "namespace retirement")?
        {
            return Ok(false);
        }
        if !self.supplies.finish().map_err(|_| "lease retirement")? {
            return Ok(false);
        }
        if let Some(identity) = self.identity.as_mut() {
            if !identity
                .finish(services)
                .map_err(|_| "identity retirement")?
            {
                return Ok(false);
            }
            self.identity = None;
            return Ok(false);
        }
        if let Some(signal) = self.ready {
            if release(signal) {
                self.ready = None;
            }
            return Ok(false);
        }
        Ok(true)
    }
}

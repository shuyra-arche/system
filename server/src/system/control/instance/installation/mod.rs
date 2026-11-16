//! Instance installation owns identity, publication, leases and readiness roots.
//! Control owns execution; Operator and Identity remain the live authorities.
pub(super) mod deliveries;
mod namespace;
use crate::system::{
    app::{
        namespace::Layout,
        services::{Services, give, release},
    },
    control::identity::{Installed, Roster},
    declaration::Unit,
    supply::{Leases, Registry},
};
use alloc::sync::Arc;
use alloc::vec::Vec;
use deliveries::Delivery;
use kernel_api::model::{
    identity::{Mark, PendId, TaskId},
    modifiable::Modifiable,
    pend::{CloneRights, Distributable, Limits, Operations, PendScope},
    sign::SignRights,
    wait::Wait,
};
use kernel_client::call::sign;
use namespace::Installation as Namespaces;
use system_protocol::identity as id;

pub(crate) struct Plan<'a> {
    pub instance: u64,
    pub task: TaskId,
    pub owner: TaskId,
    pub unit: Option<&'a Arc<Unit>>,
    pub identity: id::Install,
}
#[derive(Default)]
pub(crate) struct Installation {
    identity: Option<Installed>,
    pub namespaces: Namespaces,
    supplies: Leases,
    deliveries: Vec<Delivery>,
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
            owner,
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
        .map_err(|error| {
            use kernel_api::abi::call::pend::PendFail;
            match error {
                PendFail::Denied => "discovery grant delivery: Denied",
                PendFail::Dead => "discovery grant delivery: Dead",
                PendFail::OutOfMemory => "discovery grant delivery: OutOfMemory",
                PendFail::NotAligned => "discovery grant delivery: NotAligned",
                PendFail::HandedOver => "discovery grant delivery: HandedOver",
                PendFail::Busy => "discovery grant delivery: Busy",
                PendFail::Invalid => "discovery grant delivery: Invalid",
            }
        })?;
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
                .install(
                    layout,
                    services,
                    namespace::Plan {
                        instance,
                        task,
                        owner,
                        unit,
                    },
                    |mark, limits| sources.marked(mark, limits),
                )
                .map_err(|error| {
                    crate::debug::put(&alloc::format!(
                        "system: namespace installation: {error:?}\n"
                    ));
                    "namespace installation"
                })?;
        }
        self.installed = true;
        Ok(())
    }
    pub fn reserve_delivery(&mut self) -> Result<(), system_protocol::supply::Fail> {
        self.deliveries
            .try_reserve(1)
            .map_err(|_| system_protocol::supply::Fail::Full)
    }
    pub fn add_delivery(&mut self, delivery: Delivery) {
        self.deliveries.push(delivery);
    }
    pub fn delivery(&self, id: system_protocol::supply::InstallId) -> Option<&Delivery> {
        self.deliveries.iter().find(|entry| entry.id() == id)
    }
    pub fn cancel_delivery(&mut self, id: system_protocol::supply::InstallId) {
        self.deliveries
            .iter_mut()
            .find(|entry| entry.id() == id)
            .expect("accepted installation owner")
            .cancel();
    }
    pub fn slot_busy(&self, slot: system_protocol::Mark) -> bool {
        self.deliveries
            .iter()
            .any(|entry| entry.slot() == slot && !entry.released())
    }
    pub fn advance_deliveries(&mut self) -> Result<(), &'static str> {
        for entry in &mut self.deliveries {
            entry.advance().map_err(|_| "dynamic lease retirement")?;
        }
        Ok(())
    }
    pub fn deliveries_ready(&self) -> bool {
        self.deliveries.iter().all(Delivery::ready)
    }
    pub fn delivery_failed(&self) -> bool {
        self.deliveries
            .iter()
            .any(|entry| entry.required_failure().is_some())
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
        for entry in &mut self.deliveries {
            if !entry.released() {
                entry.cancel();
                entry.advance().map_err(|_| "dynamic lease retirement")?;
                return Ok(false);
            }
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

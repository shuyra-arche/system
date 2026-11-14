//! Native ingress roles own their queues and keep decoded caller authority local.
use super::services::{LIMIT_NS, Services, bound, fatal, release};
use crate::{
    protocol,
    system::{
        control::Control,
        supply::{Registry, Service as Supply},
    },
};
use alloc::vec::Vec;
use kernel_api::model::{
    clone_view::CloneLimits,
    identity::{Mark, PendId},
    modifiable::Modifiable,
    pend::{CloneRights, Distributable, Limits, Operations, PendScope},
    wait::Wait,
};
use kernel_client::call::{clone_view, room};
use system_protocol::{control as api, identity, loader, supply};
use weave::{Codec, Contract};

struct ControlFace {
    root: PendId,
    role: api::Grant,
    ingress: protocol::Control,
}
pub(crate) struct Endpoints {
    control: Vec<ControlFace>,
    constructor: (PendId, protocol::Loader),
    export: (PendId, protocol::Export),
    supply: (PendId, protocol::Supply),
}
fn queue<T: Codec>(services: &Services, name: &str) -> PendId {
    let max_len = T::SIZE
        .maximum()
        .unwrap_or_else(|| fatal("system: unbounded service protocol"));
    clone_view::create(
        services.license,
        Mark::of(name),
        CloneLimits {
            max_len,
            max_messages: 16,
            max_bytes: max_len.checked_mul(16).unwrap(),
        },
    )
    .unwrap_or_else(|_| fatal("system: service queue creation failed"))
}
fn maximum() -> Limits {
    Limits {
        operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
        modifiable: Modifiable::Yes,
        distributable: Distributable::No,
        scope: PendScope::Object,
    }
}
impl Endpoints {
    pub fn new(services: &Services, sources: &mut Registry) -> Self {
        let mut control = Vec::new();
        control
            .try_reserve_exact(7)
            .unwrap_or_else(|_| fatal("system: control ingress capacity"));
        for role in [
            api::Grant::State,
            api::Grant::Mint,
            api::Grant::Embark,
            api::Grant::Debark,
            api::Grant::Ruin,
            api::Grant::Observe,
            api::Grant::Recover,
        ] {
            let root = queue::<<api::Call as Contract>::Request>(
                services,
                &alloc::format!("system/control/{}", role.name()),
            );
            let ingress = protocol::Control::bind(root, services.license, services.context.clone())
                .unwrap_or_else(|_| fatal("system: control ingress binding failed"));
            sources.register("system/control", role.name(), root, maximum());
            control.push(ControlFace {
                root,
                role,
                ingress,
            });
        }
        let root = queue::<<loader::Call as Contract>::Request>(services, "system/control/create");
        let constructor = (
            root,
            protocol::Loader::bind(root, services.license, services.context.clone())
                .unwrap_or_else(|_| fatal("system: construction ingress binding failed")),
        );
        sources.register("system/control", "create", root, maximum());
        let root = queue::<<identity::export::Call as Contract>::Request>(
            services,
            "system/identity/export",
        );
        let export = (
            root,
            protocol::Export::bind(root, services.license, services.context.clone())
                .unwrap_or_else(|_| fatal("system: export ingress binding failed")),
        );
        sources.register("system/identity", "export", root, maximum());
        let root = queue::<<supply::Call as Contract>::Request>(services, "system/supply");
        let supply = (
            root,
            protocol::Supply::bind(root, services.license, services.context.clone())
                .unwrap_or_else(|_| fatal("system: supply ingress binding failed")),
        );
        sources.register("system", "supply", root, maximum());
        for grant in identity::Grant::ALL {
            if grant.mount() != identity::Mount::Installer {
                sources.register(
                    "system/identity",
                    grant.name(),
                    services.identity_entry(grant),
                    maximum(),
                );
            }
        }
        Self {
            control,
            constructor,
            export,
            supply,
        }
    }
    pub fn advance(
        &mut self,
        services: &Services,
        control: &mut Control,
        supply: &mut Supply,
        sources: &Registry,
    ) {
        for face in &mut self.control {
            let role = face.role;
            let result = face.ingress.serve_once(Wait::Poll, |caller, request| {
                control.command(services, caller, role, request)
            });
            check(result);
        }
        check(
            self.constructor
                .1
                .serve_once(Wait::Poll, |caller, request| {
                    control.build(services, caller, request)
                }),
        );
        check(
            self.export
                .1
                .serve_once(Wait::Poll, |caller, request| match request {
                    identity::export::operations::DecodedRequest::Export(request) => {
                        identity::export::operations::Response::Export(
                            control.export(services, caller, request),
                        )
                    }
                }),
        );
        check(self.supply.1.serve_once(Wait::Poll, |caller, request| {
            supply.answer(sources, caller, &control.supply_policy(caller), request)
        }));
    }
    pub fn finish(&mut self) {
        let deadline = room::clock().saturating_add(LIMIT_NS);
        macro_rules! close {
            ($ingress:expr, $root:expr) => {{
                while $ingress.finish().is_err() {
                    crate::runtime::heap::poll();
                    bound(deadline, "system: ingress retirement timeout");
                }
                while !release($root) {
                    crate::runtime::heap::poll();
                    bound(deadline, "system: queue retirement timeout");
                }
            }};
        }
        for face in &mut self.control {
            close!(face.ingress, face.root);
        }
        close!(self.constructor.1, self.constructor.0);
        close!(self.export.1, self.export.0);
        close!(self.supply.1, self.supply.0);
    }
}
fn check(result: Result<(), protocol::Error>) {
    match result {
        Ok(()) | Err(protocol::Error::NotReady) | Err(protocol::Error::TimedOut) => (),
        // Invalid frames/reply routes and failed replies have no second dispatch.
        // Retained receive/return obligations remain inside this live endpoint.
        Err(protocol::Error::Serve(_)) => (),
        Err(_) => fatal("system: ingress state lost"),
    }
}

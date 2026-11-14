//! Initial-Task composition. Services and Control keep their own mutable facts.
pub(crate) mod boot;
pub(crate) mod bootstrap;
mod endpoints;
pub(crate) mod events;
pub(crate) mod namespace;
pub(crate) mod services;
pub(crate) mod waiting;
pub(crate) mod worker;
use crate::{
    boot::{Boot, Resource},
    system::{
        control::Control,
        declaration::Catalog,
        loader::Loader,
        supply::{Registry, Service as Supply},
    },
};
use kernel_api::{
    boot::ledger::{Call, Name, Page},
    model::identity::PendId,
};
use kernel_client::call::room;
use services::{LIMIT_NS, Services, bound, fatal};

pub fn run(boot: Boot) -> Result<(), usize> {
    let archive =
        unit_contract::bundle::Archive::parse(boot.accounts.view()).map_err(|_| 3usize)?;
    let catalog = Catalog::decode(archive).map_err(|_| 3usize)?;
    let license = boot.accounts.permission(Call::Create).ok_or(1usize)?;
    let Some(Resource::Nest(space)) = boot.accounts.resource(Name::Space) else {
        return Err(1);
    };
    // SAFETY: both constructors receive this initial Task's actual Space from
    // the trusted Kernel ledger, before any external Unit is resumed.
    let mut services = unsafe { Services::new(license, space) };
    services.start();
    events::emit(unit_contract::event::Event::Started {
        system: services.caller.get() as u64,
    });
    let loader = unsafe { Loader::new(license, space) }.map_err(|_| 4usize)?;
    let mut control = Control::new(
        catalog,
        boot.catalog,
        loader,
        boot.accounts.control(Call::Guard),
    )
    .map_err(|_| 4usize)?;
    let mut sources = Registry::default();
    for (name, page) in [
        ("dtb", Page::Dtb),
        ("initrd", Page::Initrd),
        ("memory", Page::Memory),
    ] {
        if let Some(root) = boot.accounts.page(page) {
            sources.register("kernel/boot", name, root, services::inspect(root).limits);
        }
    }
    // Register only the genuine Kernel ledger grants. Names describe source
    // coordinates; no device type or physical address creates authority.
    for entry in boot.accounts.entries() {
        if let Name::Page(Page::Region(base)) = entry.name() {
            let root = PendId::mint(entry.handle());
            sources.register(
                "kernel/boot",
                &alloc::format!("region-{base:x}"),
                root,
                services::inspect(root).limits,
            );
        }
    }
    let mut endpoints = endpoints::Endpoints::new(&services, &mut sources);
    let mut supply = Supply::default();
    let mut closing = None;
    let started = room::clock();
    let result = loop {
        crate::runtime::heap::poll();
        services.health();
        endpoints.advance(&services, &mut control, &mut supply, &sources);
        if supply.advance().is_err() {
            break Err(4);
        }
        if control.progress(&services, &sources).is_err() {
            break Err(4);
        }
        if closing.is_none() && control.eligible() {
            control.begin_close();
            closing = Some(room::clock());
        }
        if closing.is_some() && control.closed() {
            break Ok(());
        }
        if !control.startup_complete() && room::clock().saturating_sub(started) >= 60_000_000_000 {
            break Err(3);
        }
        if closing.is_some_and(|since| room::clock().saturating_sub(since) >= 10_000_000_000) {
            break Err(9);
        }
        room::yield_();
    };
    // An error does not authorize an early Return of source storage. Close
    // construction and retain all execution/install owners through retirement.
    control.begin_close();
    let deadline = room::clock().saturating_add(LIMIT_NS * 2);
    while !control.closed() {
        crate::runtime::heap::poll();
        if control.progress(&services, &sources).is_err() {
            fatal("system: instance retirement failed")
        }
        bound(deadline, "system: instance retirement unsettled");
    }
    while !supply
        .finish()
        .unwrap_or_else(|_| fatal("system: supply retirement failed"))
    {
        crate::runtime::heap::poll();
        bound(deadline, "system: supply retirement timeout");
    }
    crate::debug::put(&alloc::format!(
        "system: retiring {} cached images\n",
        control.cached_entries()
    ));
    while !control.finish_cache() {
        crate::runtime::heap::poll();
        bound(deadline, "system: image cache retirement timeout");
    }
    endpoints.finish();
    services.finish();
    events::emit(unit_contract::event::Event::Complete {
        system: services.caller.get() as u64,
        reason: result.as_ref().err().copied().unwrap_or(0) as u64,
    });
    result
}

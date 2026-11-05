use super::Declarations;
use super::supplies::Supplies;
use super::{boot, life, policy, wait};
use crate::boot::Boot;
use crate::system::{assembly, control, identity, loader, operator};
use ::schedule::Resources;
use alloc::vec::Vec;
use env::{Call, Name};

pub(crate) fn resources(
    boot: Boot,
    declarations: Declarations,
) -> Result<Resources<'static>, crate::system::app::InstallError> {
    super::config::validate(declarations.list())
        .map_err(crate::system::app::InstallError::Configuration)?;
    let status = boot::status();
    let inputs = declarations
        .list()
        .iter()
        .filter(|program| {
            program.name() != declarations.system()
                && program.control.creation == unit::document::Creation::Startup
        })
        .map(|program| {
            let program = program.clone();
            let image = boot
                .catalog
                .find(program.loader.image)
                .map(|entry| entry.bytes)
                .ok_or(crate::system::app::InstallError::Configuration(
                    "deployment start image missing",
                ))?;
            Ok(crate::system::control::unit::start::Input {
                program,
                image: Some(image),
            })
        })
        .collect::<Result<Vec<_>, crate::system::app::InstallError>>()?;
    let deployed = inputs
        .iter()
        .map(|input| input.program.clone())
        .collect::<Vec<_>>();
    let guarded = deployed
        .iter()
        .filter(|unit| unit.control.guarded)
        .map(|unit| unit.name())
        .collect::<Vec<_>>();
    let mut groups = alloc::collections::BTreeMap::<&'static str, Vec<&'static str>>::new();
    for unit in &deployed {
        for group in &unit.control.groups {
            groups.entry(group).or_default().push(unit.name());
        }
    }
    let targets = groups
        .into_iter()
        .map(|(name, members)| crate::unit::Target { name, members })
        .collect::<Vec<_>>();
    let guard = super::guard::authority(&guarded, boot.accounts.token(Name::Call(Call::Guard)))
        .map_err(crate::system::app::InstallError::Configuration)?;
    let mut resources = Resources::new();
    resources.insert(boot.catalog)?.insert(
        Supplies::new(boot.machine, boot.accounts, boot.catalog)
            .map_err(crate::system::app::InstallError::Configuration)?,
    )?;
    resources.insert(declarations.clone())?;
    resources.insert(super::operator::Layout::default())?;
    resources.insert(super::operator::BootstrapReceipt::new(declarations.clone()))?;
    control::install(
        &mut resources,
        status.clone(),
        control::Configuration {
            inputs,
            targets,
            guarded,
            guard,
        },
    )?;
    identity::install(&mut resources)?;
    operator::install(&mut resources)?;
    resources.insert(identity::export::Exports::default())?;
    loader::install(&mut resources)?;
    assembly::install(&mut resources, declarations)?;
    let namespaces = crate::system::assembly::namespace::NamespaceInstallations::new();
    resources.insert(namespaces)?;

    resources
        .insert(status)?
        .insert(boot::Faces::default())?
        .insert(boot.machine)?
        .insert(policy::Settle::new(
            policy::Flow {
                settling: false,
                forced: false,
                done: false,
            },
            &deployed,
        ))?
        .insert(policy::Activity {
            quiet: env::chrono::clock(),
            walking: false,
        })?
        .insert(policy::Bound(env::Wait::POLL))?
        .insert(policy::Shutoff(None))?
        .insert(wait::Waiting::new()?)?
        .insert(wait::Interests {
            tokens: Vec::new(),
            writes: Vec::new(),
            subs: Vec::new(),
            armed: false,
        })?
        .insert(life::Deadline(0))?;
    Ok(resources)
}

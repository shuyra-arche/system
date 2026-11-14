//! Native Identity installation receipts, owned by the instance coordinator.
use crate::system::app::{
    namespace::result,
    services::{Services, give},
};
use alloc::vec::Vec;
use kernel_api::model::{
    identity::{Mark, PendId, TaskId},
    modifiable::Modifiable,
    pend::{CloneRights, Distributable, Limits, Operations, PendScope},
};
use system_protocol::{identity as api, value::Outcome};

#[derive(Default)]
pub(crate) struct Roster {
    next: u64,
}
pub(crate) struct Installed {
    id: api::InstallId,
}
impl Roster {
    pub fn install(
        &mut self,
        services: &Services,
        task: TaskId,
        install: api::Install,
    ) -> Result<Installed, api::Fail> {
        self.next = self.next.checked_add(1).ok_or(api::Fail::Full)?;
        let id = api::InstallId::new(self.next).ok_or(api::Fail::Full)?;
        let status = result(services.identity::<api::operations::Install>(
            api::Grant::Install,
            api::InstallRequest {
                id,
                task: task.into(),
                install,
            },
        ))?;
        if status.state.0 != api::InstallState::Installed {
            return Err(api::Fail::Denied);
        }
        Ok(Installed { id })
    }

    pub fn grant(
        &self,
        services: &Services,
        task: TaskId,
        grants: &[api::Grant],
    ) -> Result<Vec<PendId>, api::Fail> {
        let mut delivered = Vec::new();
        delivered
            .try_reserve_exact(grants.len() + 1)
            .map_err(|_| api::Fail::Full)?;
        for grant in core::iter::once(&api::Grant::Resolve)
            .chain(grants.iter())
            .copied()
        {
            if grant == api::Grant::Resolve && !delivered.is_empty() {
                continue;
            }
            if grant.mount() == api::Mount::Installer {
                return Err(api::Fail::Denied);
            }
            delivered.push(
                give(
                    services.identity_entry(grant),
                    task,
                    Mark::of(&alloc::format!("system/identity/{}", grant.name())),
                    Limits {
                        operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
                        modifiable: Modifiable::Yes,
                        distributable: Distributable::No,
                        scope: PendScope::Object,
                    },
                )
                .map_err(|_| api::Fail::Denied)?,
            );
        }
        Ok(delivered)
    }
    pub fn subject(&self, services: &Services, task: TaskId) -> Result<api::Subject, api::Fail> {
        match services.identity::<api::operations::Resolve>(
            api::Grant::Resolve,
            api::Task { task: task.into() },
        ) {
            Outcome::Success(binding) => binding
                .0
                .map(|binding| binding.current)
                .ok_or(api::Fail::Denied),
            Outcome::Failure(error) => Err(error),
        }
    }
}
impl Installed {
    pub fn finish(&mut self, services: &Services) -> Result<bool, api::Fail> {
        let status =
            result(services.identity::<api::operations::Retire>(api::Grant::Install, self.id))?;
        Ok(matches!(
            status.state.0,
            api::InstallState::Retired | api::InstallState::Closed | api::InstallState::Cancelled
        ))
    }
}

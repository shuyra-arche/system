//! Retain the provider request and actual safe-retirement receipt before Stop.
use crate::system::app::{
    namespace::Layout,
    services::{Services, release},
};
use alloc::vec::Vec;
use kernel_api::model::identity::{PendId, TaskId};
use system_protocol::{operator as op, retirement as api, value::Outcome};
use unit_contract::document::Retirement;

struct Participant {
    declaration: Retirement<&'static str>,
    target: TaskId,
    root: Option<PendId>,
    pending: Option<system_client::Pending<api::operations::Retire>>,
    cleanup: Option<system_client::Cleanup>,
    settled: bool,
}
#[derive(Default)]
pub(crate) struct Gate {
    participants: Vec<Participant>,
}
impl Gate {
    pub fn new(target: TaskId, declarations: &[Retirement<&'static str>]) -> Self {
        Self {
            participants: declarations
                .iter()
                .cloned()
                .map(|declaration| Participant {
                    declaration,
                    target,
                    root: None,
                    pending: None,
                    cleanup: None,
                    settled: false,
                })
                .collect(),
        }
    }
    pub fn advance(
        &mut self,
        services: &Services,
        layout: &Layout,
        mut anchor: impl FnMut(&str, usize) -> Option<op::Entry>,
    ) -> bool {
        for part in &mut self.participants {
            if let Some(cleanup) = part.cleanup.as_mut() {
                if cleanup.finish().is_err() {
                    return false;
                }
                part.cleanup = None;
            }
            if part.settled {
                if let Some(root) = part.root {
                    if !release(root) {
                        return false;
                    }
                    part.root = None;
                }
                continue;
            }
            if let Some(call) = part.pending.as_mut() {
                match call.poll() {
                    Ok(Some(response)) => {
                        if let Outcome::Success(receipt) = response.value {
                            part.settled =
                                receipt.task.get() == part.target.get() as u64 && receipt.settled();
                        }
                        part.cleanup = Some(part.pending.take().unwrap().into_cleanup());
                    }
                    Ok(None) => (),
                    Err(_) => {
                        part.cleanup = Some(part.pending.take().unwrap().into_cleanup());
                    }
                }
                return false;
            }
            if part.root.is_none() {
                let Some(base) = anchor(
                    part.declaration.provider,
                    usize::from(part.declaration.namespace),
                ) else {
                    return false;
                };
                match layout.resource(services, base, part.declaration.face) {
                    Ok(resource) => part.root = PendId::try_from(resource.token).ok(),
                    Err(_) => return false,
                }
            }
            let Some(root) = part.root else { return false };
            let info = crate::system::app::services::inspect(root);
            let Ok(mut client) = system_client::retirement::Client::bind(
                root,
                info.owner,
                services.license,
                services.context.clone(),
            ) else {
                return false;
            };
            match client.begin::<api::operations::Retire>(
                api::Target {
                    task: part.target.into(),
                },
                kernel_api::model::wait::Wait::At(
                    kernel_client::call::room::clock().saturating_add(1_000_000_000),
                ),
            ) {
                Ok(call) => part.pending = Some(call),
                Err(failure) => part.cleanup = Some(failure.cleanup),
            }
            return false;
        }
        true
    }
}

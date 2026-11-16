// One Instance retains namespace grants, publications and cleanup receipts.
use crate::system::{
    app::{
        namespace::{Layout, result},
        services::{self, Services, give, inspect, release},
    },
    declaration::{self, Namespace},
};
use alloc::vec::Vec;
use kernel_api::model::{
    identity::{Mark, PendId, TaskId},
    pend::Distributable,
};
use system_protocol::{
    operator as op,
    value::{Optional, Rights},
};
use unit_contract::document::Placement;

/// All acquired grants, publications and private delegation roots remain here,
/// including when a later setup step fails. Entry coordinates are observations.
pub(crate) struct Plan<'a> {
    pub instance: u64,
    pub task: TaskId,
    pub owner: TaskId,
    pub unit: &'a declaration::Unit,
}
#[derive(Default)]
pub(crate) struct Installation {
    pub anchors: Vec<op::Entry>,
    grants: Vec<op::GrantId>,
    bindings: Vec<op::Binding>,
    containers: Vec<op::Binding>,
    roots: Vec<PendId>,
    removal: Option<op::OperationId>,
    // Last acknowledged Operator revision, used only as a CAS precondition.
    root: Option<op::unit::Root>,
}
impl Installation {
    pub fn root(&self) -> Option<op::unit::Root> {
        self.root.clone()
    }
    pub fn phase(&mut self, services: &Services, phase: op::unit::Phase) -> Result<(), op::Fail> {
        let Some(root) = &self.root else {
            return Ok(());
        };
        if root.phase == phase {
            return Ok(());
        }
        self.root = Some(result(
            services.mutate::<op::authority::operations::AdvanceRoot>(op::unit::Advance {
                instance: root.registration.instance,
                expected: root.revision,
                phase,
            }),
        )?);
        Ok(())
    }
    pub fn install(
        &mut self,
        layout: &mut Layout,
        services: &Services,
        plan: Plan<'_>,
        source: impl Fn(Mark, kernel_api::model::pend::Limits) -> Option<PendId>,
    ) -> Result<(), op::Fail> {
        let Plan {
            instance,
            task,
            owner,
            unit,
        } = plan;
        self.anchors
            .try_reserve_exact(unit.namespaces().len())
            .map_err(|_| op::Fail::Full)?;
        self.grants
            .try_reserve_exact(unit.namespaces().len())
            .map_err(|_| op::Fail::Full)?;
        self.containers
            .try_reserve_exact(unit.namespaces().len())
            .map_err(|_| op::Fail::Full)?;
        let publications: usize = unit
            .namespaces()
            .iter()
            .map(|ns| ns.publications.len())
            .sum();
        self.bindings
            .try_reserve_exact(publications)
            .map_err(|_| op::Fail::Full)?;
        self.roots
            .try_reserve_exact(publications)
            .map_err(|_| op::Fail::Full)?;
        if unit
            .namespaces()
            .iter()
            .any(|ns| ns.placement == Placement::Instance)
        {
            self.containers.try_reserve(1).map_err(|_| op::Fail::Full)?;
            let pane = layout.part(
                services,
                layout.root(services),
                &alloc::format!("i{instance}"),
            )?;
            self.containers.push(pane.clone());
            let info = services::lookup(task).ok_or(op::Fail::Dead)?;
            if info.creator != services.caller
                || info.state == kernel_api::model::unit::UnitState::Exited
            {
                return Err(op::Fail::Denied);
            }
            let team = info.team;
            self.root = Some(result(
                services.mutate::<op::authority::operations::RegisterRoot>(
                    op::unit::Registration {
                        instance: system_protocol::control::instance::InstanceId {
                            system: services.caller.into(),
                            sequence: system_protocol::control::instance::Sequence::new(instance)
                                .ok_or(op::Fail::Bad)?,
                        },
                        owner: owner.into(),
                        task: task.into(),
                        team: team.into(),
                        pane,
                    },
                ),
            )?);
        }
        let mut keys: Vec<(&str, op::Entry)> = Vec::new();
        keys.try_reserve_exact(unit.namespaces().len())
            .map_err(|_| op::Fail::Full)?;
        for ns in unit.namespaces() {
            let mut anchor = if ns.placement == Placement::Instance {
                let root = self
                    .root
                    .as_ref()
                    .ok_or(op::Fail::Unknown)?
                    .registration
                    .pane
                    .entry
                    .clone();
                // Composition keys remain policy, below this instance's one
                // ordinary UnitRoot Pane rather than a global type hierarchy.
                if let Some((_, anchor)) = keys.iter().find(|(key, _)| *key == ns.key) {
                    anchor.clone()
                } else {
                    let container = layout.part(services, root, ns.key)?;
                    let anchor = container.entry.clone();
                    self.containers.push(container);
                    keys.push((ns.key, anchor.clone()));
                    anchor
                }
            } else {
                layout.key(services, ns.key)?
            };
            if ns.placement == Placement::Instance && !ns.path.as_str().is_empty() {
                self.containers
                    .try_reserve(ns.path.as_str().split('/').count())
                    .map_err(|_| op::Fail::Full)?;
                for name in ns.path.as_str().split('/') {
                    let child = layout.part(services, anchor, name)?;
                    anchor = child.entry.clone();
                    self.containers.push(child);
                }
            } else {
                anchor = layout.path(services, anchor, &ns.path)?;
            }
            self.publish(layout, services, anchor.clone(), ns, &source)?;
            let id = op::GrantId::new(layout.number()).unwrap();
            // The pending grant's identity belongs to this installation before
            // the effect. An indeterminate reply terminates System with owners.
            self.grants.push(id);
            let request = op::GrantSpec {
                id,
                parent: Optional(None),
                holder: task.into(),
                anchor: anchor.clone(),
                scope: ns.scope.clone(),
                ops: ns.operations,
                permit: Optional(ns.permit.clone()),
                source: Optional(Some(task.into())),
                source_mark: Optional(ns.source_mark.map(Into::into)),
            };
            let deadline = services::clock().saturating_add(services::LIMIT_NS);
            loop {
                let granted =
                    result(services.mutate::<op::authority::operations::Grant>(request.clone()))?;
                match granted.state.0 {
                    op::GrantState::Granted => break,
                    op::GrantState::Pending => {
                        services::bound(deadline, "system: namespace grant timeout")
                    }
                    _ => return Err(op::Fail::Denied),
                }
            }
            self.anchors.push(anchor);
        }
        Ok(())
    }
    fn publish(
        &mut self,
        layout: &mut Layout,
        services: &Services,
        anchor: op::Entry,
        ns: &Namespace,
        source: &impl Fn(Mark, kernel_api::model::pend::Limits) -> Option<PendId>,
    ) -> Result<(), op::Fail> {
        for publication in &ns.publications {
            let original = source(publication.mark, publication.limits).ok_or(op::Fail::Unknown)?;
            let meta = inspect(original);
            let mut limits = publication.limits;
            limits.distributable = Distributable::Yes;
            let mut request = kernel_api::model::pend::AccordRequest {
                src: original,
                target: kernel_api::model::pend::AccordTarget::Caller,
                mark: publication.mark,
                limits,
            }
            .to_bytes();
            let root = services::accord(kernel_api::address::VirtAddr::new(
                request.as_mut_ptr().addr(),
            ))
            .map_err(|_| op::Fail::Denied)?;
            self.roots.push(root);
            let delivered = give(root, services.operator(), publication.mark, limits)
                .map_err(|_| op::Fail::Denied)?;
            let binding = result(
                services.mutate::<op::authority::operations::Bind>(op::Bind {
                    change: op::Mutation {
                        id: op::OperationId::new(layout.number()).unwrap(),
                        entry: anchor.clone(),
                        expected: 0,
                    },
                    name: publication.name.clone(),
                    resource: delivered.into(),
                    resource_id: meta.target.words()[1] as u64,
                    rights: Rights(publication.limits),
                    permit: publication.permit.clone(),
                    source: op::Source {
                        task: meta.owner.into(),
                        mark: publication.mark.into(),
                    },
                }),
            )?;
            self.bindings.push(binding);
        }
        Ok(())
    }
    pub fn finish(&mut self, layout: &mut Layout, services: &Services) -> Result<bool, op::Fail> {
        if let Some(id) = self.grants.last().copied() {
            let status = result(services.mutate::<op::authority::operations::Revoke>(id))?;
            if status.state.0 == op::GrantState::Closed {
                self.grants.pop();
            }
            return Ok(false);
        }
        if let Some(binding) = self.bindings.last() {
            let id = *self
                .removal
                .get_or_insert_with(|| op::OperationId::new(layout.number()).unwrap());
            let status = result(services.mutate::<op::authority::operations::Unbind>(
                op::Mutation {
                    id,
                    entry: binding.entry.clone(),
                    expected: binding.revision,
                },
            ))?;
            if status.state.0 == op::OperationState::Applied {
                self.bindings.pop();
                self.removal = None;
            }
            return Ok(false);
        }
        if let Some(root) = self.roots.last().copied() {
            if release(root) {
                self.roots.pop();
            }
            return Ok(false);
        }
        if let Some(binding) = self.containers.last() {
            let id = *self
                .removal
                .get_or_insert_with(|| op::OperationId::new(layout.number()).unwrap());
            let status = result(services.mutate::<op::authority::operations::Unpart>(
                op::Mutation {
                    id,
                    entry: binding.entry.clone(),
                    expected: binding.revision,
                },
            ))?;
            if status.state.0 == op::OperationState::Applied {
                self.containers.pop();
                self.removal = None;
            }
            return Ok(false);
        }
        self.anchors.clear();
        Ok(true)
    }
}

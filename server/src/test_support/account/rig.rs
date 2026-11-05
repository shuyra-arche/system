use super::hook_fault::HookFault;
use super::hooks::hooks;
use super::signals::Signals;
use crate::system::app::operator::Layout;
use crate::system::app::Fault;
use crate::system::assembly::namespace::NamespaceInstallations;
use crate::system::control::identity::{self, Roster};
use crate::system::control::unit::table::State as UnitState;
use crate::system::control::unit::Control;
use crate::system::operator::management::Tree;
use ::schedule::{Cursor, Plan, Progress, Schedule};
use alloc::sync::Arc;
use env::{TaskId, Wait};
use system_client::operator::Capability;

pub struct AccountRig<'a> {
    fixture: &'a mut super::super::Fixture,
    plan: Option<Plan<Fault>>,
    cursor: Cursor,
    observed: usize,
    user: Option<system_api::identity::PrincipalId>,
}

impl<'a> AccountRig<'a> {
    pub fn new(fixture: &'a mut super::super::Fixture) -> Self {
        Self {
            fixture,
            plan: None,
            cursor: Cursor::default(),
            observed: 0,
            user: None,
        }
    }

    pub fn authority(&self) -> env::TaskId {
        self.fixture
            .resources
            .read::<Roster>()
            .unwrap()
            .authority()
            .unwrap()
    }

    pub fn account_task(&self) -> TaskId {
        self.fixture
            .resources
            .read::<Control>()
            .unwrap()
            .task("account")
            .unwrap()
    }

    pub fn instance_entry(&self) -> env::PieToken {
        crate::system::control::instance_entry(&self.fixture.resources).unwrap()
    }

    pub fn construction_entry(&self) -> env::PieToken {
        self.fixture
            .resources
            .read::<crate::system::control::Construction>()
            .unwrap()
            .entries()
            .next()
            .unwrap()
    }

    pub fn prepare_login(
        &mut self,
        login: &'static unit::UnitFile,
        worker: TaskId,
        peer: TaskId,
        inherited: TaskId,
    ) -> system_api::identity::PrincipalId {
        {
            let mut system = self.fixture.resources.write::<Control>().unwrap();
            system.fixture_attach_unit(login, worker).unwrap();
            system.register_internal(peer).unwrap();
            system.register_internal(inherited).unwrap();
        }
        let mut roster = self
            .fixture
            .resources
            .write::<Roster>()
            .unwrap();
        roster.authorize(worker).unwrap();
        roster.authorize(peer).unwrap();
        roster.inherit(inherited, worker).unwrap();
        let login = identity::binding(&roster, worker).unwrap().unwrap().current;
        let outsider = identity::binding(&roster, peer).unwrap().unwrap().current;
        assert_ne!(login.principal, outsider.principal);
        let child = identity::binding(&roster, inherited)
            .unwrap()
            .unwrap()
            .current;
        assert_eq!(child.principal, login.principal);
        login.principal
    }

    pub fn connect_tasks(&mut self, tasks: [TaskId; 3]) {
        for task in tasks {
            crate::system::operator::connect_task(&self.fixture.resources, task).unwrap();
        }
    }

    pub fn operator_context(&self) -> (Capability, u64) {
        let start = self
            .fixture
            .resources
            .read::<Layout>()
            .unwrap()
            .as_ref()
            .unwrap()
            .reserve_operations(2)
            .unwrap()
            .0;
        let info = self
            .fixture
            .resources
            .read::<Tree>()
            .unwrap()
            .root_entry()
            .unwrap();
        let capability = Capability::direct(info.entry.operator, info.entrance).unwrap();
        (capability, start)
    }

    pub fn install_hooks(&mut self, signals: Arc<Signals>) {
        self.fixture
            .resources
            .insert(HookFault {
                signals: Some(signals),
                ..HookFault::default()
            })
            .unwrap();
        let mut schedule = Schedule::new();
        schedule
            .add_plan("control.poll", 0u8, crate::system::control::poll().unwrap())
            .unwrap();
        schedule
            .add_system(
                "construction.receive",
                0u8,
                crate::system::control::receive_construction,
            )
            .unwrap();
        schedule
            .add_system(
                "construction.admit",
                1,
                crate::system::control::admit_construction,
            )
            .unwrap();
        schedule
            .add_plan("launch", 2, crate::system::launch::frame().unwrap())
            .unwrap();
        schedule
            .add_plan("assembly.jobs", 3, crate::system::assembly::frame().unwrap())
            .unwrap();
        schedule
            .add_system(
                "instances.receive",
                5,
                crate::system::control::receive_instances,
            )
            .unwrap();
        schedule
            .add_system(
                "instances.answer",
                6,
                crate::system::control::answer_instances,
            )
            .unwrap();
        schedule
            .add_system(
                "instances.reap",
                7,
                crate::system::control::instance::schedule::reap,
            )
            .unwrap();
        schedule.add_plan("instance.hooks", 8, hooks()).unwrap();
        schedule
            .add_system("launch.completed", 10, crate::system::assembly::completed)
            .unwrap();
        let mut plan = schedule.build().unwrap();
        plan.prepare(&self.fixture.resources);
        self.plan = Some(plan);
    }

    pub fn drive(
        &mut self,
        worker: TaskId,
        peer: TaskId,
        signals: &Signals,
        login_subject: system_api::identity::PrincipalId,
        creator: &'static [unit::Construction],
    ) {
        let plan = self.plan.as_mut().expect("account hook plan");
        signals.entry.store(1, core::sync::atomic::Ordering::Release);
        signals
            .inherited_ready
            .store(true, core::sync::atomic::Ordering::Release);
        let deadline = env::chrono::clock() + 15_000_000_000;
        loop {
            if signals.creator_request.load(core::sync::atomic::Ordering::Acquire)
                && !signals.creator_ready.load(core::sync::atomic::Ordering::Acquire)
            {
                self.fixture
                    .resources
                    .write::<crate::system::control::Construction>()
                    .unwrap()
                    .grant(peer, creator)
                    .unwrap();
                signals
                    .creator_ready
                    .store(true, core::sync::atomic::Ordering::Release);
            }
            self.fixture.progress().unwrap();
            if plan.advance(&mut self.cursor, &self.fixture.resources).unwrap() == Progress::Done {
                self.cursor.reset();
            }
            let stage = signals.stage.load(core::sync::atomic::Ordering::Acquire);
            let mut target = TaskId::new(signals.target.load(core::sync::atomic::Ordering::Acquire));
            if stage == 5 && stage != self.observed {
                self.fixture
                    .resources
                    .write::<HookFault>()
                    .unwrap()
                    .prepare = true;
                self.observed = stage;
                signals
                    .ack
                    .store(stage, core::sync::atomic::Ordering::Release);
                continue;
            }
            if stage == 15 {
                target = self
                    .fixture
                    .resources
                    .read::<HookFault>()
                    .unwrap()
                    .failed_task
                    .expect("preparation failure was not injected");
                let control = self.fixture.resources.read::<Control>().unwrap();
                if !control
                    .instance(target)
                    .is_some_and(|item| item.state == UnitState::Dead)
                {
                    continue;
                }
            }
            if stage != 0 && stage != self.observed {
                let roster = self
                    .fixture
                    .resources
                    .write::<Roster>()
                    .unwrap();
                let binding = identity::binding(&roster, target).unwrap();
                let runtime = self
                    .fixture
                    .resources
                    .read::<NamespaceInstallations>()
                    .unwrap()
                    .prepared(target);
                if stage < 10 {
                    let subject = binding
                        .expect("account-instance: Held task has no identity")
                        .current;
                    assert_ne!(subject.principal, login_subject);
                    assert!(
                        runtime,
                        "account-instance: create returned before runtime registration"
                    );
                    if let Some(principal) = self.user {
                        assert_eq!(principal, subject.principal);
                    }
                    self.user = Some(subject.principal);
                    if stage == 3 {
                        env::room::doom(target).unwrap();
                    }
                } else {
                    assert!(binding.is_none(), "account-instance: Dead task still bound");
                    assert!(!runtime, "account-instance: Dead task still registered");
                }
                self.observed = stage;
                signals
                    .ack
                    .store(stage, core::sync::atomic::Ordering::Release);
            }
            let reclaimed = self
                .fixture
                .resources
                .read::<Control>()
                .unwrap()
                .instances()
                .all(|item| item.settled());
            let registered = self
                .fixture
                .resources
                .read::<NamespaceInstallations>()
                .unwrap()
                .prepared(target);
            if signals.done.load(core::sync::atomic::Ordering::Acquire)
                && signals
                    .inherited_done
                    .load(core::sync::atomic::Ordering::Acquire)
                && env::unit::join_task(worker, Wait::POLL).unwrap_or(true)
                && reclaimed
                && !registered
            {
                break;
            }
            assert!(
                env::chrono::clock() < deadline,
                "account-instance: fixture timed out"
            );
            execution::room::park(core::time::Duration::from_millis(1)).unwrap();
        }
    }

    pub fn finish(
        &mut self,
        worker: TaskId,
        peer: TaskId,
        inherited: TaskId,
        signals: &Signals,
    ) {
        self.fixture
            .resources
            .write::<Control>()
            .unwrap()
            .detach_unit("login")
            .expect("detach login unit");
        {
            let mut roster = self
                .fixture
                .resources
                .write::<Roster>()
                .unwrap();
            roster.unbind(worker).unwrap();
            roster.unbind(peer).unwrap();
            roster.unbind(inherited).unwrap();
            let target = TaskId::new(signals.target.load(core::sync::atomic::Ordering::Acquire));
            assert!(
                identity::binding(&roster, target)
                    .unwrap()
                    .is_none()
            );
        }
        self.fixture.progress().unwrap();
        {
            let fault = self.fixture.resources.read::<HookFault>().unwrap();
            assert!(
                fault.waited && fault.retired_failed && fault.failed_task.is_some(),
                "hook failure coverage missing"
            );
        }
    }
}

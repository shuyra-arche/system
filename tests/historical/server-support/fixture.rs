use super::SceneError;
use super::action::FixtureAction;
use super::error::FixtureError;
use super::fault::FaultResource;
use super::instance_view::InstanceView;
use super::state::FixtureState;
use super::supply_hook::SupplyHook;
use super::supply_resource::SupplyResource;
use crate::boot::Boot;
use crate::system::app::{self, Declarations, Fault, schedule};
use crate::system::assembly::{self, Jobs};
use crate::system::control::identity::Roster;
use crate::system::control::{
    lifecycle::{Action, Active, Directive, Key, Origin, Request, schedule as lifecycle},
    unit::verdict,
    unit::{Control, start},
};
use ::schedule::{
    Cursor, Dispatch, Invocation, Plan, Progress, Res, ResMut, Resources as Registry, Schedule,
};
use alloc::vec::Vec;
use env::TaskId;
use unit::UnitFile;

fn supply(
    active: Res<Active>,
    roster: Res<Roster>,
    control: Res<Control>,
    hook: Res<SupplyResource>,
) -> Result<Progress, verdict::Fail> {
    let job = active.0.as_ref().ok_or(verdict::Fail::Unknown)?;
    let program = control.input(&job.key.request.name)?.program;
    let task = control
        .task(&job.key.request.name)
        .ok_or(verdict::Fail::Unknown)?;
    (hook.0)(roster.authority(), program, task).map_err(|_| verdict::Fail::NotReady)?;
    Ok(Progress::Done)
}

fn probe(
    fault: Res<FaultResource>,
    mut dispatch: ResMut<Dispatch<(), verdict::Fail>>,
) -> Result<Progress, verdict::Fail> {
    dispatch
        .begin(usize::from(fault.armed))
        .map_err(|_| verdict::Fail::NotReady)?;
    Ok(Progress::Done)
}

fn select_probe(
    mut dispatch: ResMut<Dispatch<(), verdict::Fail>>,
) -> Result<Progress, verdict::Fail> {
    dispatch
        .select(Invocation {
            key: (),
            cursor: Default::default(),
        })
        .map_err(|_| verdict::Fail::NotReady)?;
    Ok(Progress::Done)
}

fn inject(
    active: Res<Active>,
    resources: Res<assembly::namespace::NamespaceInstallations>,
    mut fault: ResMut<FaultResource>,
    mut dispatch: ResMut<Dispatch<(), verdict::Fail>>,
) -> Result<Progress, verdict::Fail> {
    dispatch
        .take_result()
        .map_err(|_| verdict::Fail::NotReady)?;
    if fault.armed {
        fault.armed = false;
        fault.task = active
            .0
            .as_ref()
            .and_then(|job| job.progress.execution.task);
        fault.prepared = fault.task.is_some_and(|task| resources.prepared(task));
        return Err(verdict::Fail::NotReady);
    }
    Ok(Progress::Done)
}

pub struct Fixture {
    pub(super) resources: Registry<'static>,
    plans: [Plan<Fault>; 4],
    cursors: [Cursor; 4],
}

impl Fixture {
    pub fn scene(
        boot: &Boot,
        declarations: &'static [&'static UnitFile],
        deployment: &'static unit::Deployment,
    ) -> Result<Vec<&'static UnitFile>, SceneError> {
        app::scene::programs(&boot.catalog, Declarations(declarations), deployment)
    }

    pub fn new(
        boot: Boot,
        declarations: &'static [&'static UnitFile],
        deployment: &'static unit::Deployment,
        supply_hook: SupplyHook,
    ) -> Result<Self, FixtureError> {
        let mut resources = app::install::resources(boot, Declarations(declarations), deployment)
            .map_err(|why| {
            crate::debug::put(&alloc::format!("system: install {}", why));
            FixtureError::Room
        })?;
        resources
            .insert(SupplyResource(supply_hook))
            .map_err(|_| FixtureError::Room)?;
        resources
            .insert(FaultResource {
                armed: false,
                task: None,
                prepared: false,
            })
            .map_err(|_| FixtureError::Room)?;
        resources
            .insert(Dispatch::<(), verdict::Fail>::new())
            .map_err(|_| FixtureError::Room)?;
        let mut start = schedule::startup().map_err(|_| FixtureError::Room)?;
        let mut children =
            lifecycle::lifecycle(assembly::static_plan::hooks().map_err(|_| FixtureError::Room)?)
                .map_err(|_| FixtureError::Room)?;
        let at = children
            .iter()
            .position(|(key, _)| *key == Key::Embark)
            .ok_or(FixtureError::Room)?;
        let (_, embark) = children.remove(at);
        let mut plan = Schedule::new();
        plan.add_system("fixture.supply", 0u8, supply)
            .map_err(|_| FixtureError::Room)?;
        plan.add_plan("embark", 1, embark)
            .map_err(|_| FixtureError::Room)?;
        plan.add_system("fixture.probe", 2, probe)
            .map_err(|_| FixtureError::Room)?;
        plan.add_subplans(
            "fixture.failure",
            3,
            select_probe,
            alloc::vec![(
                (),
                schedule::maintenance()
                    .map_err(|_| FixtureError::Room)?
                    .map_error(|_| verdict::Fail::NotReady)
            )],
            inject,
        )
        .map_err(|_| FixtureError::Room)?;
        children.push((Key::Embark, plan.build().map_err(|_| FixtureError::Room)?));
        let mut plans = [
            schedule::maintenance()
                .map_err(|_| FixtureError::Room)?
                .map_error(|_| Fault::Room),
            lifecycle::actions(children).map_err(|_| FixtureError::Room)?,
            schedule::frame().map_err(|_| FixtureError::Room)?,
            schedule::shutdown().map_err(|_| FixtureError::Room)?,
        ];
        start.prepare(&resources);
        for plan in &mut plans {
            plan.prepare(&resources);
        }
        start
            .advance(&mut Cursor::default(), &resources)
            .map_err(|why| {
                crate::debug::put(&alloc::format!("fixture startup: {why:?}"));
                FixtureError::Room
            })?;
        Ok(Self {
            resources,
            plans,
            cursors: core::array::from_fn(|_| Cursor::default()),
        })
    }

    pub fn assemble(&mut self, program: &UnitFile) -> Result<(), unit::Died> {
        self.action(program.name(), FixtureAction::Mint)
            .map_err(|_| start::E_PROGRAM)?;
        self.action(program.name(), FixtureAction::Embark { parent: None })
            .map_err(|_| start::E_PROGRAM)?;
        Ok(())
    }

    pub fn action(
        &mut self,
        name: &str,
        action: FixtureAction,
    ) -> Result<Option<TaskId>, FixtureError> {
        let action = match action {
            FixtureAction::Mint => Action::Mint,
            FixtureAction::Embark { parent } => Action::Embark { parent },
            FixtureAction::Ruin => Action::Ruin,
            FixtureAction::Debark => Action::Debark,
        };
        let job = self
            .resources
            .write::<Jobs>()
            .map_err(|_| FixtureError::Room)?
            .submit_lifecycle(Request {
                name: name.into(),
                directive: Directive {
                    action,
                    origin: Origin::External,
                },
                back: None,
            })
            .map_err(|_| FixtureError::Full)?;
        loop {
            if self.plans[1]
                .advance(&mut self.cursors[1], &self.resources)
                .map_err(|_| FixtureError::Room)?
                == Progress::Done
            {
                self.cursors[1].reset();
            }
            self.progress().map_err(|_| FixtureError::Room)?;
            {
                let mut operations = self
                    .resources
                    .write::<Jobs>()
                    .map_err(|_| FixtureError::Room)?;
                if let Some(result) = operations.take_local(job) {
                    return result
                        .map(|(_, task)| task)
                        .map_err(|_| FixtureError::NotReady);
                }
            }
            execution::room::park(core::time::Duration::from_millis(1))
                .map_err(|_| FixtureError::Room)?;
        }
    }

    pub fn mint(&mut self, name: &str) -> Result<TaskId, FixtureError> {
        self.action(name, FixtureAction::Mint)?
            .ok_or(FixtureError::NotReady)
    }

    pub fn embark(&mut self, name: &str) -> Result<(), FixtureError> {
        self.action(name, FixtureAction::Embark { parent: None })?;
        Ok(())
    }

    pub fn progress(&mut self) -> Result<(), &'static str> {
        if self.plans[0]
            .advance(&mut self.cursors[0], &self.resources)
            .map_err(|_| "fixture maintenance")?
            == Progress::Done
        {
            self.cursors[0].reset();
        }
        Ok(())
    }

    pub fn settle(&mut self) {
        self.resources
            .write::<app::policy::Settle>()
            .unwrap()
            .flow
            .settling = true;
    }

    pub fn supervise(&mut self) -> Result<(), FixtureError> {
        while !self
            .resources
            .read::<app::policy::Settle>()
            .map_err(|_| FixtureError::Room)?
            .flow
            .done
        {
            if self.plans[2]
                .advance(&mut self.cursors[2], &self.resources)
                .map_err(|error| {
                    crate::debug::put(&alloc::format!("fixture: frame {:?}", error));
                    FixtureError::Shutdown
                })?
                == Progress::Done
            {
                self.cursors[2].reset();
            }
        }
        while self.plans[3]
            .advance(&mut self.cursors[3], &self.resources)
            .map_err(|_| FixtureError::Shutdown)?
            == Progress::Pending
        {}
        Ok(())
    }

    pub fn active_hook_task(&self) -> Result<Option<TaskId>, FixtureError> {
        Ok(self
            .resources
            .read::<Jobs>()
            .map_err(|_| FixtureError::Room)?
            .active_hook_task())
    }

    pub fn prepared(&self, task: TaskId) -> Result<bool, FixtureError> {
        Ok(self
            .resources
            .read::<assembly::namespace::NamespaceInstallations>()
            .map_err(|_| FixtureError::Room)?
            .prepared(task))
    }

    pub fn roster_authority(&self) -> Result<Option<TaskId>, FixtureError> {
        Ok(self
            .resources
            .read::<Roster>()
            .map_err(|_| FixtureError::Room)?
            .authority())
    }

    pub fn instance(&self, name: &str) -> Result<Option<InstanceView>, FixtureError> {
        let control = self
            .resources
            .read::<Control>()
            .map_err(|_| FixtureError::Room)?;
        let Some(task) = control.task(name) else {
            return Ok(None);
        };
        self.instance_by_task(task)
    }

    pub fn instance_by_task(&self, task: TaskId) -> Result<Option<InstanceView>, FixtureError> {
        let control = self
            .resources
            .read::<Control>()
            .map_err(|_| FixtureError::Room)?;
        control
            .instances()
            .find(|item| item.task == task)
            .map(|item| self.view(item))
            .transpose()
    }

    pub fn instances(&self) -> Result<Vec<InstanceView>, FixtureError> {
        self.resources
            .read::<Control>()
            .map_err(|_| FixtureError::Room)?
            .instances()
            .map(|item| self.view(item))
            .collect()
    }

    fn view(
        &self,
        item: &crate::system::control::instance::state::Instance,
    ) -> Result<InstanceView, FixtureError> {
        Ok(InstanceView {
            task: item.task,
            state: match item.state {
                crate::system::control::unit::table::State::NeverStarted => {
                    FixtureState::NeverStarted
                }
                crate::system::control::unit::table::State::Starting => FixtureState::Starting,
                crate::system::control::unit::table::State::Ready => FixtureState::Ready,
                crate::system::control::unit::table::State::Stopping => FixtureState::Stopping,
                crate::system::control::unit::table::State::Debarked => FixtureState::Debarked,
                crate::system::control::unit::table::State::Dead => FixtureState::Dead,
            },
            team: item.team(),
            prepared: self.prepared(item.task)?,
        })
    }
}

//! One instance owns its execution attempt and all installation/reclamation work.
use crate::system::{
    app::{
        namespace::Layout,
        services::{Services, lookup, observe},
    },
    assembly::{Gate, Installation},
    control::identity::Roster,
    declaration::Unit,
    loader::{Job, Loader},
    supply::Registry,
};
use alloc::sync::Arc;
use kernel_api::model::{
    identity::TaskId,
    unit::{UnitQuery, UnitState},
};
use system_protocol::{
    control::{Fail, Observation, State},
    identity,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InstanceId(core::num::NonZeroU64);
impl InstanceId {
    pub fn new(value: u64) -> Option<Self> {
        core::num::NonZeroU64::new(value).map(Self)
    }
    pub fn get(self) -> u64 {
        self.0.get()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Intent {
    Dormant,
    Run,
    Freeze,
    Stop,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Building,
    Installing,
    Prepared,
    Started,
    Retiring,
    Uninstalling,
    Reclaiming,
    Retired,
}

pub(crate) struct Instance {
    pub id: InstanceId,
    pub owner: TaskId,
    pub program: Option<Arc<Unit>>,
    job: Job,
    installation: Installation,
    identity: identity::Install,
    gate: Gate,
    stage: Stage,
    intent: Intent,
    failure: Option<Fail>,
    last: Observation,
    ready: bool,
    exited: bool,
    supplied: bool,
}
impl Instance {
    pub fn new(
        id: InstanceId,
        owner: TaskId,
        program: Option<Arc<Unit>>,
        job: Job,
        identity: identity::Install,
        start: bool,
    ) -> Self {
        Self {
            id,
            owner,
            program,
            job,
            installation: Installation::default(),
            identity,
            gate: Gate::default(),
            stage: Stage::Building,
            intent: if start { Intent::Run } else { Intent::Dormant },
            failure: None,
            last: Observation {
                state: State::Starting,
                reason: 0,
                completed: false,
                task: system_protocol::TaskId::NONE,
                team: system_protocol::TeamId::NONE,
            },
            ready: false,
            exited: false,
            supplied: false,
        }
    }
    pub fn task(&self) -> Option<TaskId> {
        self.job.task()
    }
    pub fn anchor(&self, index: usize) -> Option<system_protocol::operator::Entry> {
        self.installation.namespaces.anchors.get(index).cloned()
    }
    pub fn retired(&self) -> bool {
        self.stage == Stage::Retired
    }
    pub fn constructed(&self) -> bool {
        self.supplied && matches!(self.stage, Stage::Prepared | Stage::Started)
    }
    pub fn failure(&self) -> Option<Fail> {
        self.failure
    }
    pub fn observation(&self) -> Observation {
        self.last.clone()
    }
    pub fn ready(&self) -> bool {
        self.stage == Stage::Started && self.ready && self.intent == Intent::Run
    }
    pub fn contains_caller(&self, caller: TaskId) -> bool {
        !self.retired()
            && self.job.team().is_some_and(|team| {
                lookup(caller)
                    .is_some_and(|info| info.team == team && info.state != UnitState::Exited)
            })
    }
    pub fn owns_caller(&self, caller: TaskId) -> bool {
        !self.retired() && (self.owner == caller || self.contains_caller(caller))
    }
    pub fn request(&mut self, intent: Intent) -> Result<(), Fail> {
        if self.retired() {
            return Err(Fail::NotReady);
        }
        if self.intent == Intent::Stop && intent != Intent::Stop {
            return Err(Fail::NotReady);
        }
        if intent == Intent::Freeze && self.stage != Stage::Started {
            return Err(Fail::NotReady);
        }
        self.intent = intent;
        Ok(())
    }
    pub fn progress(
        &mut self,
        loader: &mut Loader,
        services: &Services,
        roster: &mut Roster,
        layout: &mut Layout,
        sources: &Registry,
        anchors: &[(
            alloc::string::String,
            alloc::vec::Vec<system_protocol::operator::Entry>,
        )],
    ) -> Result<bool, &'static str> {
        if self.stage == Stage::Retired {
            return Ok(false);
        }
        if let Some(task) = self.job.task() {
            let info = self
                .job
                .observe()
                .map_err(|_| "instance observation")?
                .unwrap();
            self.last.task = task.into();
            self.last.team = info.team.into();
            self.last.reason = info.exit.map_or(0, |(_, reason)| reason);
            if !self.exited
                && let Some((cause, reason)) = info.exit
            {
                use kernel_api::model::unit::ExitCause;
                use unit_contract::event::{Cause, Event, Exit};
                crate::system::app::events::emit(Event::Exited {
                    task: task.get() as u64,
                    exit: Exit {
                        cause: match cause {
                            ExitCause::Reap => Cause::Reap,
                            ExitCause::Slay => Cause::Slay,
                            ExitCause::Cascade => Cause::Cascade,
                            ExitCause::Fault => Cause::Fault,
                        },
                        reason,
                    },
                });
                crate::debug::put(&alloc::format!(
                    "system: Task {} exit result={:?} reclaimed={}\n",
                    task.get(),
                    info.result,
                    info.reclaimed
                ));
                self.exited = true;
            }
            if info.state == UnitState::Exited && self.stage == Stage::Started {
                self.intent = Intent::Stop;
            }
            if self.owner != services.caller
                && lookup(self.owner).is_none_or(|info| info.state == UnitState::Exited)
            {
                self.intent = Intent::Stop;
            }
        }
        if self.intent == Intent::Stop
            && !matches!(
                self.stage,
                Stage::Retiring | Stage::Uninstalling | Stage::Reclaiming
            )
        {
            if let Some(task) = self.job.task() {
                self.gate = Gate::new(
                    task,
                    self.program
                        .as_ref()
                        .map_or(&[][..], |unit| unit.control().retirement.as_slice()),
                );
            }
            self.stage = Stage::Retiring;
        }
        match self.stage {
            Stage::Building => match self.job.advance(loader) {
                Ok(true) => {
                    if self.job.phase() != crate::system::loader::Phase::Dormant {
                        return Err("invalid physical construction phase");
                    }
                    let task = self.job.task().ok_or("constructed Task missing")?;
                    self.gate = Gate::new(
                        task,
                        self.program
                            .as_ref()
                            .map_or(&[][..], |unit| unit.control().retirement.as_slice()),
                    );
                    crate::system::app::events::emit(unit_contract::event::Event::Bound {
                        unit: self.program.as_ref().map_or("", |unit| unit.name()),
                        task: task.get() as u64,
                        team: self.job.team().unwrap().get() as u64,
                    });
                    self.stage = Stage::Installing;
                }
                Ok(false) => (),
                Err(_) => {
                    self.failure = Some(Fail::BadImage);
                    self.intent = Intent::Stop;
                }
            },
            Stage::Installing => {
                let task = self.job.task().ok_or("installation Task missing")?;
                if let Err(error) = self.installation.install(
                    services,
                    roster,
                    layout,
                    sources,
                    crate::system::assembly::Plan {
                        instance: self.id.get(),
                        task,
                        unit: self.program.as_ref(),
                        identity: self.identity.clone(),
                    },
                ) {
                    crate::debug::put(&alloc::format!(
                        "system: instance {} installation: {error}\n",
                        self.id.get()
                    ));
                    self.failure = Some(Fail::Denied);
                    self.intent = Intent::Stop;
                } else {
                    self.stage = Stage::Prepared;
                }
            }
            Stage::Prepared => {
                let prepared = match self.installation.prepared(services) {
                    Ok(prepared) => prepared,
                    Err(_) => {
                        self.failure = Some(Fail::Denied);
                        self.intent = Intent::Stop;
                        false
                    }
                };
                self.supplied = prepared;
                if prepared && self.intent == Intent::Run {
                    self.job.resume().map_err(|_| "instance start")?;
                    self.stage = Stage::Started;
                }
            }
            Stage::Started => {
                let task = self.job.task().ok_or("running Task missing")?;
                let info = observe(UnitQuery::Target(task.into()));
                if self.intent == Intent::Freeze && info.state != UnitState::Frozen {
                    kernel_client::call::unit::freeze(
                        self.job.team().ok_or("running Team missing")?.into(),
                    )
                    .map_err(|_| "instance freeze")?;
                } else if self.intent == Intent::Run && info.state == UnitState::Frozen {
                    kernel_client::call::unit::resume(
                        self.job.team().ok_or("running Team missing")?.into(),
                    )
                    .map_err(|_| "instance resume")?;
                }
                match self.installation.ready() {
                    Ok(ready) => {
                        if ready && !self.ready {
                            crate::system::app::events::emit(unit_contract::event::Event::Ready {
                                task: task.get() as u64,
                            });
                        }
                        self.ready = ready;
                    }
                    Err(_) => {
                        self.failure = Some(Fail::NotReady);
                        self.intent = Intent::Stop;
                    }
                }
            }
            Stage::Retiring => {
                if self.gate.advance(services, layout, |name, index| {
                    anchors
                        .iter()
                        .find(|(unit, _)| unit == name)
                        .and_then(|(_, values)| values.get(index))
                        .cloned()
                }) {
                    self.job.cancel().map_err(|_| "instance stop")?;
                    self.stage = Stage::Uninstalling;
                }
            }
            Stage::Uninstalling => {
                if let Some(task) = self.job.task()
                    && !observe(UnitQuery::Target(task.into())).reclaimed
                {
                    return Ok(false);
                }
                if self.installation.finish(layout, services)? {
                    self.stage = Stage::Reclaiming;
                }
            }
            Stage::Reclaiming => match self.job.finish() {
                Ok(true) => {
                    if let Some(info) = self.job.result() {
                        if !info.reclaimed {
                            return Err("retired Task reclamation unproven");
                        }
                        self.last.reason = info.exit.map_or(0, |(_, reason)| reason);
                    }
                    if self.last.task != system_protocol::TaskId::NONE {
                        crate::system::app::events::emit(unit_contract::event::Event::Retired {
                            task: self.last.task.get(),
                        });
                    }
                    self.stage = Stage::Retired;
                    self.last.completed = true;
                }
                Ok(false) => (),
                Err(error) if error.busy() => (),
                Err(_) => return Err("instance backing retirement"),
            },
            Stage::Retired => (),
        }
        self.last.state = match self.stage {
            Stage::Building | Stage::Installing => State::Starting,
            Stage::Prepared => State::Debarked,
            Stage::Started if self.intent == Intent::Freeze => State::Debarked,
            Stage::Started if self.ready => State::Ready,
            Stage::Started => State::Starting,
            Stage::Retiring | Stage::Uninstalling | Stage::Reclaiming => State::Stopping,
            Stage::Retired => State::Dead,
        };
        Ok(true)
    }
}

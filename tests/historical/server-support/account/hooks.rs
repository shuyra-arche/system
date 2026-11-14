use super::hook_fault::HookFault;
use crate::system::app::Fault as Fail;
use crate::system::assembly::Jobs;
use crate::system::control::instance::hook;
use crate::system::control::unit::Control;
use crate::system::control::unit::table::State as UnitState;
use ::schedule::{Plan, Progress, Res, ResMut, Schedule};
use core::sync::atomic::Ordering;

fn fail_prepare(
    mut fault: ResMut<HookFault>,
    active: Res<Jobs>,
    resources: Res<crate::system::assembly::namespace::NamespaceInstallations>,
) -> Result<Progress, &'static str> {
    if fault.prepare {
        let task = active.active_hook_task().ok_or("probe hook target")?;
        assert!(
            resources.prepared(task),
            "prepare hook failed before mounting runtime"
        );
        fault.prepare = false;
        fault.failed_task = Some(task);
        return Err("probe preparation hook failure");
    }
    Ok(Progress::Done)
}

fn delay_retire(
    mut fault: ResMut<HookFault>,
    active: Res<Jobs>,
    control: Res<Control>,
) -> Result<Progress, &'static str> {
    let item = control
        .instances()
        .find(|item| Some(item.task) == active.active_hook_task())
        .unwrap();
    assert_eq!(item.state, UnitState::Stopping);
    assert!(
        item.team().is_some(),
        "retire hook reported reclaimed before completion"
    );
    if !fault.waited {
        fault.waited = true;
        fault.waiting = Some(
            fault
                .signals
                .as_ref()
                .unwrap()
                .pings
                .load(Ordering::Acquire),
        );
        return Ok(Progress::Pending);
    }
    if let Some(baseline) = fault.waiting {
        if fault
            .signals
            .as_ref()
            .unwrap()
            .pings
            .load(Ordering::Acquire)
            <= baseline
        {
            return Ok(Progress::Pending);
        }
        fault.waiting = None;
    }
    if !fault.retired_failed {
        fault.retired_failed = true;
        return Err("probe retirement hook failure");
    }
    Ok(Progress::Done)
}

pub(super) fn hooks() -> Plan<Fail> {
    let children = crate::system::assembly::hooks::children().unwrap();
    let mut wrapped = alloc::vec::Vec::new();
    for (key, child) in children {
        let mut wrap = Schedule::new();
        if key == hook::Key::Retire {
            wrap.add_system("delay", 0u8, delay_retire).unwrap();
        }
        wrap.add_plan("registered", 1u8, child).unwrap();
        if key == hook::Key::Prepare {
            wrap.add_system("fail", 2u8, fail_prepare).unwrap();
        }
        wrapped.push((key, wrap.build().unwrap()));
    }
    hook::plan(wrapped).unwrap()
}

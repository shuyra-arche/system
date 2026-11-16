use unit_contract::{
    document as d,
    event::{Cause, Event},
};

pub fn scenario(value: &str) -> bool {
    matches!(
        value,
        "handoff"
            | "auth"
            | "death"
            | "cancel"
            | "owner"
            | "owner-cancel"
            | "owner-death"
            | "supply"
            | "supply-contend"
            | "install"
            | "install-loss"
    )
}

pub fn catalog(scenario: &str) -> Vec<d::v2::Unit<String>> {
    let mut catalog = vec![
        d::v2::Unit {
            schema: 2,
            id: "controller".into(),
            loader: d::Loader {
                image: "user-bin".into(),
                stack: 512 * 1024,
                args: vec![match scenario {
                    "death" => 2,
                    "cancel" => 4,
                    "auth" => 6,
                    _ => 0,
                }],
                ..Default::default()
            },
            identity: d::Identity {
                aliases: vec![],
                grants: vec![1],
            },
            control: d::Control {
                ending: d::Ending::Transient,
                construction: vec![d::Construction {
                    unit: Some("driver".into()),
                    owner: d::Owner::Caller,
                    identity: d::IdentitySelection::Caller,
                    arguments: d::Arguments { min: 1, max: 1 },
                    stack: d::Range {
                        min: 512 * 1024,
                        max: 512 * 1024,
                    },
                    max_image: 0,
                }],
                ..Default::default()
            },
            supply: d::v2::Supply {
                bindings: vec![d::v2::SupplyBinding {
                    source: "system/control".into(),
                    resource: "instance".into(),
                    slot: 7,
                    rights: d::Rights {
                        operations: d::rights::Operations::Clone(vec![
                            d::rights::Clone::Push,
                            d::rights::Clone::Peek,
                        ]),
                        modifiable: true,
                        distributable: false,
                        window: d::rights::Window::Inherit,
                    },
                }],
            },
            ..Default::default()
        },
        d::v2::Unit {
            schema: 2,
            id: "driver".into(),
            loader: d::Loader {
                image: "user-bin".into(),
                stack: 512 * 1024,
                ..Default::default()
            },
            control: d::Control {
                creation: d::Creation::Requested,
                ready: d::Ready::Signal(0xabcd),
                ending: d::Ending::Transient,
                ..Default::default()
            },
            operator: d::v2::Operator {
                namespace: vec![d::v2::Namespace {
                    key: "driver".into(),
                    placement: d::Placement::Instance,
                    path: "".into(),
                    scope: d::Scope::Subtree,
                    ops: (1 << 2) | (1 << 4) | (1 << 5) | (1 << 8),
                    permit: None,
                    source_mark: None,
                    bindings: vec![],
                }],
            },
            ..Default::default()
        },
    ];
    if matches!(scenario, "supply" | "supply-contend") {
        catalog[0].loader.args = vec![14];
        catalog[0].control.ready = d::Ready::Signal(0xc0de);
        let mut service = catalog[0].supply.bindings[0].clone();
        service.source = "system".into();
        service.resource = "supply".into();
        service.slot = 8;
        catalog[0].supply.bindings.push(service);
        let mut state = catalog[0].supply.bindings[0].clone();
        state.resource = "state".into();
        state.slot = 10;
        catalog[0].supply.bindings.push(state);
        catalog[1].supply.bindings.push(d::v2::SupplyBinding {
            source: "controller".into(),
            resource: "signal".into(),
            slot: 0xbeef,
            rights: d::Rights {
                operations: d::rights::Operations::Sign(vec![d::rights::Sign::Trigger]),
                modifiable: false,
                distributable: false,
                window: d::rights::Window::Inherit,
            },
        });
        let mut self_binding = catalog[1].supply.bindings[0].clone();
        self_binding.slot = 0xbef0;
        catalog[1].supply.bindings.push(self_binding);
        let mut consumer = catalog[0].supply.bindings[1].clone();
        consumer.slot = 9;
        catalog[1].supply.bindings.push(consumer);
    }
    if scenario == "supply-contend" {
        let mut pressure = catalog[0].clone();
        pressure.id = "pressure".into();
        pressure.loader.args = vec![19];
        pressure.control.ready = d::Ready::Spawn;
        pressure.control.construction.clear();
        catalog.insert(1, pressure);
    }
    if matches!(scenario, "install" | "install-loss") {
        catalog[0].loader.args = vec![if scenario == "install-loss" { 18 } else { 16 }];
        let mut service = catalog[0].supply.bindings[0].clone();
        service.source = "system".into();
        service.resource = "supply".into();
        service.slot = 8;
        catalog[0].supply.bindings.push(service.clone());
        service.slot = 9;
        catalog[1].supply.bindings.push(service);
        // No Driver device binding: Create must complete before the source
        // exists, and Install selects the actual observed Prepared Task.
    }
    if scenario == "auth" {
        let mut observer = catalog[0].clone();
        observer.id = "observer".into();
        observer.loader.args = vec![3];
        observer.control.construction.clear();
        catalog.insert(1, observer);
    }
    if matches!(scenario, "owner" | "owner-cancel" | "owner-death") {
        catalog[0].loader.args = vec![match scenario {
            "owner" => 7,
            "owner-cancel" => 10,
            _ => 12,
        }];
        catalog[0].control.construction[0].owner = d::Owner::Selected;
        catalog[0].control.construction[0].stack.max = 1024 * 1024;
        let mut state = catalog[0].supply.bindings[0].clone();
        state.resource = "state".into();
        state.slot = 8;
        catalog[0].supply.bindings.push(state);
        let mut owner = catalog[0].clone();
        owner.id = "owner".into();
        owner.loader.args = vec![match scenario {
            "owner" => 8,
            "owner-cancel" => 11,
            _ => 13,
        }];
        owner.control.construction.clear();
        catalog.insert(1, owner);
    }
    catalog
}
pub fn check(scenario: &str, events: &[Event<String>], log: &[u8]) -> bool {
    if !self::scenario(scenario) {
        return false;
    }
    let Some(Event::Started { system }) = events.first() else {
        panic!("missing System identity")
    };
    assert!(
        matches!(events.last(), Some(Event::Complete { system: owner, reason: 0 }) if owner == system)
    );
    let serial = String::from_utf8_lossy(log);
    assert!(!serial.contains("FAIL handoff") && !serial.contains("page fault killed"));
    let marker = match scenario {
        "install-loss" => "PASS handoff install-loss=1 start-gate=1 cleanup=1",
        "install" => {
            "PASS handoff post-prepared=1 target=1 replay=1 cancel=1 isolation=1 withdraw=1 retire=1"
        }
        "supply" | "supply-contend" => {
            "PASS handoff provider=1 register=1 installed=1 replay=1 revoke=1 ancestor=1 rebind=1"
        }
        "death" => "PASS handoff owner-death accepted=1",
        "cancel" => "PASS handoff lost-reply=1 pending-cancel=1",
        "auth" => "PASS handoff create=1 replay=1 conflict=1 root-grants=1 stale=1",
        "owner" => "PASS handoff selected-owner creator-exit=1 lifecycle=1 root=1",
        "owner-cancel" => "PASS handoff selected-cancel requester=1 cleanup=1",
        "owner-death" => "PASS handoff selected-death receipt=1 isolation=1",
        _ => {
            "PASS handoff recovery=1 instance=1 roots=1 cas=1 rpc=1 supply=1 revoke=1 stop=1 retire=1"
        }
    };
    assert!(serial.contains(marker));
    if scenario == "supply-contend" {
        assert!(serial.contains("PASS supply-contention pairs="));
    }
    if scenario == "auth" {
        assert!(serial.contains("PASS handoff authorization=1"));
    }
    let mut bound = vec![];
    for (at, event) in events.iter().enumerate() {
        if let Event::Bound { task, team, .. } = event {
            assert!(
                !bound
                    .iter()
                    .any(|(old_task, old_team)| old_task == task || old_team == team)
            );
            bound.push((*task, *team));
            let exit = events.iter().enumerate().find(|(_, event)| matches!(event, Event::Exited { task: observed, exit } if observed == task && exit.cause != Cause::Fault)).unwrap().0;
            let retired = events.iter().enumerate().find(|(_, event)| matches!(event, Event::Retired { task: observed } if observed == task)).unwrap().0;
            assert!(at < exit && exit < retired);
        }
    }
    assert!(
        bound.len()
            >= if scenario == "supply-contend" {
                4
            } else if matches!(scenario, "handoff" | "auth" | "owner" | "supply") {
                3
            } else if matches!(
                scenario,
                "owner-cancel" | "owner-death" | "install" | "install-loss"
            ) {
                2
            } else {
                1
            }
    );
    if matches!(scenario, "owner" | "owner-cancel" | "owner-death") {
        let task = |name: &str| {
            events
                .iter()
                .find_map(|event| match event {
                    Event::Bound { unit, task, .. } if unit == name => Some(*task),
                    _ => None,
                })
                .unwrap()
        };
        let exit = |task| {
            events.iter().position(|event| matches!(event, Event::Exited { task: observed, .. } if *observed == task)).unwrap()
        };
        if scenario == "owner" {
            assert!(serial.contains("PASS handoff selected-requester replay=1 isolation=1"));
            let driver = task("driver");
            let ready = events
                .iter()
                .position(|event| matches!(event, Event::Ready { task } if *task == driver))
                .unwrap();
            assert!(
                exit(task("controller")) < ready,
                "Driver must start after requester exit"
            );
        } else if scenario == "owner-cancel" {
            assert!(serial.contains("PASS handoff selected-owner cancelled=1"));
        } else {
            assert!(serial.contains("PASS handoff selected-owner-death accepted=1"));
            for event in events {
                if let Event::Bound {
                    unit,
                    task: driver_task,
                    ..
                } = event
                    && unit == "driver"
                {
                    let retired = events.iter().position(|event| matches!(event, Event::Retired { task: observed } if observed == driver_task)).unwrap();
                    assert!(exit(task("owner")) < retired);
                }
            }
        }
    }
    if scenario == "install-loss" {
        for event in events {
            if let Event::Bound { unit, task, .. } = event
                && unit == "driver"
            {
                assert!(
                    !events.iter().any(
                        |event| matches!(event, Event::Ready { task: ready } if ready == task)
                    ),
                    "failed Prepared target became Ready"
                );
            }
        }
    }
    // Typed Complete precedes the terminal native call. Require Kernel's
    // actual acceptance even after every application assertion has passed.
    let root_exit = format!("exit tid={system} reason=0x0 note: system: complete");
    if !serial.lines().any(|line| line.trim_end() == root_exit) {
        let console_lost = serial.lines().any(|line| {
            line.strip_prefix("[kernel] console-lost=")
                .and_then(|value| value.trim().parse::<u64>().ok())
                .is_some_and(|value| value != 0)
        });
        if console_lost {
            // Distinct from PASS and from a behavioral failure. The runner
            // may qualify a fresh VM, retaining this incomplete receipt.
            eprintln!("INCONCLUSIVE kernel-console-loss missing admitted root exit: {root_exit}");
            std::process::exit(2);
        }
        panic!("missing admitted root exit: {root_exit}");
    }
    println!(
        "PASS system-handoff scenario={scenario} users={} independent-teams=1 root-exit=1 retired=1",
        bound.len()
    );
    true
}

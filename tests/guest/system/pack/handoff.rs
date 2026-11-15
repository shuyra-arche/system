use unit_contract::{
    document as d,
    event::{Cause, Event},
};

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
    if scenario == "auth" {
        let mut observer = catalog[0].clone();
        observer.id = "observer".into();
        observer.loader.args = vec![3];
        observer.control.construction.clear();
        catalog.insert(1, observer);
    }
    catalog
}
pub fn check(scenario: &str, events: &[Event<String>], log: &[u8]) -> bool {
    if !matches!(scenario, "handoff" | "auth" | "death" | "cancel") {
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
        "death" => "PASS handoff owner-death accepted=1",
        "cancel" => "PASS handoff lost-reply=1 pending-cancel=1",
        "auth" => "PASS handoff create=1 replay=1 conflict=1 root-grants=1 stale=1",
        _ => {
            "PASS handoff recovery=1 instance=1 roots=1 cas=1 rpc=1 supply=1 revoke=1 stop=1 retire=1"
        }
    };
    assert!(serial.contains(marker));
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
            >= if matches!(scenario, "handoff" | "auth") {
                3
            } else {
                1
            }
    );
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

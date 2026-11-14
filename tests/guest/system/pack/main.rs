use unit_contract::{
    bundle::{self, File, Kind, Unit},
    document as d,
    event::{self, Event},
};
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args[0] == "check" {
        let log = std::fs::read(&args[1]).unwrap();
        let events: Vec<_> = log
            .split_inclusive(|byte| *byte == b'\n')
            .filter_map(|line| {
                let offset = line
                    .windows(event::PREFIX.len())
                    .position(|window| window == event::PREFIX)?;
                Some(
                    event::decode(
                        &line[offset..]
                            .iter()
                            .copied()
                            .filter(|byte| *byte != b'\r')
                            .collect::<Vec<_>>(),
                    )
                    .unwrap(),
                )
            })
            .collect();
        assert!(matches!(events.first(), Some(Event::Started { .. })));
        let expected: usize = args[2].to_str().unwrap().parse().unwrap();
        let expected_reason = if expected == 1 { 4 } else { 0 };
        assert!(
            matches!(events.last(), Some(Event::Complete { reason, .. }) if *reason == expected_reason)
        );
        let bound: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                Event::Bound { task, team, .. } => Some((*task, *team)),
                _ => None,
            })
            .collect();
        assert_eq!(bound.len(), expected);
        for (task, team) in &bound {
            assert_ne!(*task, 0);
            assert_ne!(*team, 0);
            let exited = events.iter().position(|event| matches!(event, Event::Exited { task: observed, exit } if observed == task && (expected == 1 || exit.cause == event::Cause::Reap && exit.reason == 0))).unwrap();
            let retired = events
                .iter()
                .position(
                    |event| matches!(event, Event::Retired { task: observed } if observed == task),
                )
                .unwrap();
            assert!(exited < retired);
            if expected != 1 {
                assert!(
                    String::from_utf8_lossy(&log)
                        .contains(&format!("Task {task} exit result=Some(17)"))
                );
            } else {
                assert!(!String::from_utf8_lossy(&log).contains("exit result=Some(17)"));
            }
        }
        if expected == 2 {
            assert_ne!(bound[0].0, bound[1].0);
            assert_ne!(bound[0].1, bound[1].1);
        }
        println!("PASS system-app production=1 users={expected} events=1 exit=1 retire=1");
        return;
    }
    assert_eq!(args.len(), 5);
    let capsule = std::fs::read(&args[0]).unwrap();
    let system = std::fs::read(&args[1]).unwrap();
    let image = std::fs::read(&args[2]).unwrap();
    let count: usize = args[4].to_str().unwrap().parse().unwrap();
    let mut declarations = vec![d::v2::Unit::<String> {
        schema: 2,
        id: "system".into(),
        loader: d::Loader {
            image: "system-bin".into(),
            space: d::Space::Supervisor,
            ..Default::default()
        },
        ..Default::default()
    }];
    for i in 0..count {
        declarations.push(d::v2::Unit {
            schema: 2,
            id: format!("user-{i}"),
            loader: d::Loader {
                image: "user-bin".into(),
                stack: 65536,
                args: if i == 0 { vec![] } else { vec![0xa5, 0x5a] },
                ..Default::default()
            },
            operator: d::v2::Operator {
                namespace: vec![d::v2::Namespace {
                    key: "apps".into(),
                    placement: d::Placement::Instance,
                    path: "services".into(),
                    scope: d::Scope::Subtree,
                    ops: 1 << 8,
                    permit: None,
                    source_mark: None,
                    bindings: vec![],
                }],
            },
            supply: d::v2::Supply {
                bindings: vec![d::v2::SupplyBinding {
                    source: "system/control".into(),
                    resource: if count == 1 {
                        "missing".into()
                    } else {
                        "state".into()
                    },
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
            control: d::Control {
                ending: d::Ending::Transient,
                ..Default::default()
            },
            ..Default::default()
        });
    }
    let records: Vec<_> = declarations
        .iter()
        .map(|unit| unit_contract::record::v2::encode(unit).unwrap())
        .collect();
    let units: Vec<_> = declarations
        .iter()
        .zip(&records)
        .map(|(unit, record)| Unit {
            id: &unit.id,
            record,
        })
        .collect();
    let packed = bundle::pack(
        &units,
        &[
            File {
                kind: Kind::Supervisor,
                name: "system-bin",
                bytes: &system,
            },
            File {
                kind: Kind::User,
                name: "user-bin",
                bytes: &image,
            },
        ],
        0,
        0,
        &capsule,
    )
    .unwrap();
    std::fs::write(&args[3], packed).unwrap();
}

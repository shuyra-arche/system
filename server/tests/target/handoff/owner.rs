//! Cross-Team creation keeps request correlation separate from management.
use super::*;

fn clients(
    license: PendId,
    context: Context,
) -> (
    system_client::instance::Client,
    system_client::control::Client,
) {
    let instance = find(|info| info.mark == Mark::new(7));
    let state = find(|info| info.mark == Mark::new(8));
    assert_eq!(instance.owner, state.owner);
    (
        system_client::instance::Client::bind(
            instance.token,
            instance.owner,
            license,
            context.clone(),
        )
        .unwrap(),
        system_client::control::Client::bind(state.token, state.owner, license, context).unwrap(),
    )
}

fn named(client: &mut system_client::control::Client, name: &str) -> control::Observation {
    success(finish(
        client
            .begin::<control::operations::State>(
                control::Named {
                    name: Name::new(name).unwrap(),
                },
                deadline(),
            )
            .unwrap(),
    ))
}

fn own_instance(client: &mut system_client::instance::Client) -> Option<api::Observation> {
    success(finish(
        client
            .begin::<op::Owned>(
                api::Owned {
                    after: Optional(None),
                },
                deadline(),
            )
            .unwrap(),
    ))
    .0
}

fn subject(license: PendId, context: Context) -> identity::Subject {
    let resolver = find(|info| info.mark == Mark::of("system/identity/resolve"));
    let mut client =
        system_client::identity::Client::bind(resolver.token, resolver.owner, license, context)
            .unwrap();
    let UnitTarget::Task(caller) = own().target else {
        panic!("Task");
    };
    success(finish(
        client
            .begin::<identity::operations::Resolve>(
                identity::Task {
                    task: caller.into(),
                },
                deadline(),
            )
            .unwrap(),
    ))
    .0
    .unwrap()
    .current
}

fn request(
    owner: system_protocol::TaskId,
    subject: identity::Subject,
    cancel: bool,
) -> api::CreateFor {
    api::CreateFor {
        id: id(2),
        owner,
        input: loader::Input::Unit(loader::UnitInput {
            name: Name::new("driver").unwrap(),
            stack: if cancel { 1024 * 1024 } else { 512 * 1024 },
            arguments: loader::Arguments { values: vec![9] },
        }),
        subject,
    }
}

fn denied(receipt: api::Receipt) {
    assert_eq!(receipt.state, api::State::Failed);
    assert_eq!(receipt.failure.0.unwrap().0, api::Fail::Denied);
}

pub(super) fn run(license: PendId, context: Context, role: usize) {
    let (mut client, mut state) = clients(license, context.clone());
    let caller = own();
    let UnitTarget::Task(caller_task) = caller.target else {
        panic!("Task");
    };
    if matches!(role, 7 | 10 | 12) {
        assert!(own_instance(&mut client).is_none());
        let owner = loop {
            let owner = named(&mut state, "owner");
            if owner.state == control::State::Ready {
                break owner;
            }
            query_tick(budget());
        };
        assert_ne!(owner.task, caller_task.into());
        assert_ne!(owner.team, caller.team.into());
        let request = request(owner.task, subject(license, context), role == 10);

        // A delivered Instance endpoint cannot override Selected declaration
        // policy, and an absent recipient cannot become an owner.
        denied(success(finish(
            client
                .begin::<op::Create>(
                    api::Create {
                        id: id(1),
                        input: request.input.clone(),
                        subject: request.subject.clone(),
                    },
                    deadline(),
                )
                .unwrap(),
        )));
        let mut absent = request.clone();
        absent.id = id(3);
        absent.owner = system_protocol::TaskId::NONE;
        denied(success(finish(
            client.begin::<op::CreateFor>(absent, deadline()).unwrap(),
        )));
        assert!(own_instance(&mut client).is_none());

        let accepted = success(finish(
            client
                .begin::<op::CreateFor>(request.clone(), deadline())
                .unwrap(),
        ));
        assert_eq!(accepted.state, api::State::Pending);
        let target = accepted.instance.0.unwrap();
        if role == 10 {
            let cancelling = success(finish(
                client.begin::<op::Cancel>(request.id, deadline()).unwrap(),
            ));
            let cancelled = receipt(&mut client, cancelling);
            assert_eq!(cancelled.state, api::State::Cancelled);
            assert_eq!(cancelled.instance.0, Some(target));
            assert_eq!(
                success(finish(
                    client.begin::<op::CreateFor>(request, deadline()).unwrap()
                )),
                cancelled
            );
            assert!(own_instance(&mut client).is_none());
            let _ = writeln!(
                Console,
                "PASS handoff selected-cancel requester=1 cleanup=1"
            );
            return;
        }

        let replay = success(finish(
            client
                .begin::<op::CreateFor>(request.clone(), deadline())
                .unwrap(),
        ));
        assert_eq!(replay.instance.0, Some(target));
        let mut changed = request.clone();
        changed.owner = caller_task.into();
        assert_eq!(
            finish(client.begin::<op::CreateFor>(changed, deadline()).unwrap()),
            Outcome::Failure(api::Fail::Conflict)
        );
        let terminal = receipt(&mut client, replay);
        if role == 7 {
            assert_eq!(terminal.state, api::State::Completed);
        } else {
            assert!(matches!(
                terminal.state,
                api::State::Completed | api::State::Failed
            ));
            if terminal.state == api::State::Failed {
                assert_eq!(terminal.failure.0.unwrap().0, api::Fail::Interrupted);
            }
            while named(&mut state, "owner").state != control::State::Dead {
                query_tick(budget());
            }
        }
        assert_eq!(
            finish(client.begin::<op::Inspect>(target, deadline()).unwrap()),
            Outcome::Failure(api::Fail::Denied)
        );
        denied(success(finish(
            client
                .begin::<op::Start>(
                    api::Change {
                        id: id(20),
                        instance: target,
                    },
                    deadline(),
                )
                .unwrap(),
        )));
        denied(success(finish(
            client
                .begin::<op::Stop>(
                    api::Change {
                        id: id(21),
                        instance: target,
                    },
                    deadline(),
                )
                .unwrap(),
        )));
        denied(success(finish(
            client
                .begin::<op::Retire>(
                    api::Change {
                        id: id(22),
                        instance: target,
                    },
                    deadline(),
                )
                .unwrap(),
        )));
        assert!(own_instance(&mut client).is_none());
        assert_eq!(
            success(finish(
                client.begin::<op::CreateFor>(request, deadline()).unwrap()
            )),
            terminal
        );
        let _ = writeln!(
            Console,
            "{}",
            if role == 7 {
                "PASS handoff selected-requester replay=1 isolation=1"
            } else {
                "PASS handoff selected-death receipt=1 isolation=1"
            }
        );
        return;
    }

    let mut observation = loop {
        if let Some(observation) = own_instance(&mut client) {
            break observation;
        }
        query_tick(budget());
    };
    let target = observation.instance;
    assert_eq!(
        finish(
            client
                .begin::<op::InspectOperation>(id(2), deadline())
                .unwrap()
        ),
        Outcome::Failure(api::Fail::Unknown)
    );
    if role == 13 {
        let _ = writeln!(Console, "PASS handoff selected-owner-death accepted=1");
        return;
    }
    if role == 11 {
        while observation.phase != api::Phase::Retired {
            query_tick(budget());
            observation = observe(&mut client, target);
        }
        let _ = writeln!(Console, "PASS handoff selected-owner cancelled=1");
        return;
    }
    assert_eq!(role, 8);
    // Owner selection does not confer the requester's construction policy.
    denied(success(finish(
        client
            .begin::<op::CreateFor>(
                request(caller_task.into(), subject(license, context), false),
                deadline(),
            )
            .unwrap(),
    )));
    while observation.phase != api::Phase::Prepared
        || named(&mut state, "controller").state != control::State::Dead
    {
        query_tick(budget());
        observation = observe(&mut client, target);
    }
    let root = observation.root.0.unwrap();
    assert_eq!(root.registration.owner, caller_task.into());
    assert_eq!(root.registration.instance, target);
    assert_eq!(root.registration.task, observation.task);
    let start = success(finish(
        client
            .begin::<op::Start>(
                api::Change {
                    id: id(4),
                    instance: target,
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(receipt(&mut client, start).state, api::State::Completed);
    while !observe(&mut client, target).ready {
        query_tick(budget());
    }
    let stop = success(finish(
        client
            .begin::<op::Stop>(
                api::Change {
                    id: id(5),
                    instance: target,
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(receipt(&mut client, stop).state, api::State::Completed);
    let retire = success(finish(
        client
            .begin::<op::Retire>(
                api::Change {
                    id: id(6),
                    instance: target,
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(receipt(&mut client, retire).state, api::State::Completed);
    let retired = own_instance(&mut client).unwrap();
    assert_eq!(retired.instance, target);
    assert_eq!(retired.phase, api::Phase::Retired);
    assert!(
        success(finish(
            client
                .begin::<op::Owned>(
                    api::Owned {
                        after: Optional(Some(target))
                    },
                    deadline()
                )
                .unwrap()
        ))
        .0
        .is_none()
    );
    let foreign = api::InstanceId {
        system: system_protocol::TaskId::new(999),
        sequence: target.sequence,
    };
    assert_eq!(
        finish(
            client
                .begin::<op::Owned>(
                    api::Owned {
                        after: Optional(Some(foreign))
                    },
                    deadline()
                )
                .unwrap()
        ),
        Outcome::Failure(api::Fail::Unknown)
    );
    let _ = writeln!(
        Console,
        "PASS handoff selected-owner creator-exit=1 lifecycle=1 root=1"
    );
}

pub(super) fn driver(license: PendId, context: Context) -> ! {
    let mut client = discovery(license, context);
    let root = success(finish(
        client
            .begin::<operator::operations::OwnRoot>(operator::unit::After { cursor: 0 }, deadline())
            .unwrap(),
    ))
    .0
    .unwrap();
    let UnitTarget::Task(caller) = own().target else {
        panic!("Task");
    };
    assert_eq!(root.grant.spec.holder, caller.into());
    assert_eq!(root.root.registration.task, caller.into());
    sign::trigger(find(|info| info.mark == Mark::new(READY)).token).unwrap();
    loop {
        tick(budget());
        room::sleep_until(room::clock().saturating_add(100_000_000)).unwrap();
    }
}

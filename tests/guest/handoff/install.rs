//! Public post-Prepared installation, without a declared device binding.
use super::*;
use supply::{
    Install, InstallId, InstallState, InstallStatus, Installation, Registration, RegistrationId,
    SourceState,
};
fn iid(value: u64) -> InstallId {
    InstallId::new(value).unwrap()
}
fn inspect_install(client: &mut system_client::supply::Client, id: InstallId) -> InstallStatus {
    success(finish(
        client
            .begin::<supply::operations::InspectInstall>(Installation { id }, deadline())
            .unwrap(),
    ))
}
fn settle(
    client: &mut system_client::supply::Client,
    id: InstallId,
    state: InstallState,
) -> InstallStatus {
    loop {
        let value = inspect_install(client, id);
        if value.state == state {
            return value;
        }
        query_tick(budget());
    }
}
fn pulse(signal: PendId) {
    let kernel_api::abi::call::sign::SignReply::Triggered { sequence } =
        sign::listen(signal, Wait::Poll).unwrap()
    else {
        panic!("missing installed pulse")
    };
    sign::consume(signal, sequence).unwrap();
}
pub(super) fn foreign(
    license: PendId,
    context: Context,
    instance: api::InstanceId,
    caller: TaskId,
) {
    let endpoint = find(|info| info.mark == Mark::new(9));
    let mut client =
        system_client::supply::Client::bind(endpoint.token, endpoint.owner, license, context)
            .unwrap();
    assert_eq!(
        finish(
            client
                .begin::<supply::operations::InspectInstall>(
                    Installation { id: iid(1) },
                    deadline()
                )
                .unwrap()
        ),
        Outcome::Failure(supply::Fail::Unknown)
    );
    let cancelled = success(finish(
        client
            .begin::<supply::operations::CancelInstall>(Installation { id: iid(1) }, deadline())
            .unwrap(),
    ));
    assert_eq!(cancelled.state, InstallState::Cancelled);
    let request = Install {
        id: iid(2),
        source: RegistrationId::new(1).unwrap(),
        instance,
        target: caller.into(),
        slot: system_protocol::Mark::new(DEVICE + 5),
        rights: rights(),
    };
    assert_eq!(
        finish(
            client
                .begin::<supply::operations::Install>(request, deadline())
                .unwrap()
        ),
        Outcome::Failure(supply::Fail::Denied)
    );
}
fn rights() -> Rights {
    Rights(Limits {
        operations: Operations::Sign(SignRights::TRIGGER),
        modifiable: Modifiable::No,
        distributable: Distributable::No,
        scope: PendScope::Object,
    })
}
pub(super) fn run(license: PendId, context: Context, loss: bool) {
    let endpoint = find(|info| info.mark == Mark::new(7));
    let supply_endpoint = find(|info| info.mark == Mark::new(8));
    let mut control = system_client::instance::Client::bind(
        endpoint.token,
        endpoint.owner,
        license,
        context.clone(),
    )
    .unwrap();
    let mut client = system_client::supply::Client::bind(
        supply_endpoint.token,
        supply_endpoint.owner,
        license,
        context.clone(),
    )
    .unwrap();
    let me = own();
    let UnitTarget::Task(caller) = me.target else {
        panic!("Task")
    };
    let resolver = find(|info| info.mark == Mark::of("system/identity/resolve"));
    let mut identities = system_client::identity::Client::bind(
        resolver.token,
        resolver.owner,
        license,
        context.clone(),
    )
    .unwrap();
    let subject = success(finish(
        identities
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
    .current;
    let created = success(finish(
        control
            .begin::<op::Create>(
                api::Create {
                    id: id(1),
                    input: loader::Input::Unit(loader::UnitInput {
                        name: Name::new("driver").unwrap(),
                        stack: 512 * 1024,
                        arguments: loader::Arguments { values: vec![17] },
                    }),
                    subject,
                },
                deadline(),
            )
            .unwrap(),
    ));
    let instance = created.instance.0.unwrap();
    assert_eq!(receipt(&mut control, created).state, api::State::Completed);
    let prepared = observe(&mut control, instance);
    assert_eq!(prepared.phase, api::Phase::Prepared);
    assert!(!prepared.ready);
    assert_ne!(prepared.team.get(), me.team.get() as u64);
    assert_ne!(prepared.task.get(), caller.get() as u64);
    // Sources are created only AFTER the completed Prepared observation.
    let first = sign::create(license).unwrap();
    let second = sign::create(license).unwrap();
    let mut delegation = rights().0;
    delegation.distributable = Distributable::Yes;
    let mut offers = vec![];
    for (value, signal, name) in [(1, first, "first"), (2, second, "second")] {
        let offer = provider::accord(signal, AccordTarget::Caller, delegation);
        let remote = provider::accord(offer, AccordTarget::Task(supply_endpoint.owner), delegation);
        let registered = provider::register(
            &mut client,
            supply::Register {
                id: RegistrationId::new(value).unwrap(),
                source: supply::Key {
                    name: "controller".into(),
                },
                resource: supply::Key { name: name.into() },
                offered: remote.into(),
                maximum: rights(),
            },
        );
        assert_eq!(registered.state, SourceState::Active);
        offers.push(offer);
    }
    let request = Install {
        id: iid(1),
        source: RegistrationId::new(1).unwrap(),
        instance,
        target: prepared.task,
        slot: system_protocol::Mark::new(DEVICE),
        rights: rights(),
    };
    for (changed, failure) in [
        (
            Install {
                target: caller.into(),
                ..request.clone()
            },
            supply::Fail::Denied,
        ),
        (
            Install {
                instance: api::InstanceId {
                    system: system_protocol::TaskId::new(999),
                    ..instance
                },
                ..request.clone()
            },
            supply::Fail::Unknown,
        ),
        (
            Install {
                source: RegistrationId::new(55).unwrap(),
                ..request.clone()
            },
            supply::Fail::Unknown,
        ),
        (
            Install {
                slot: Mark::of("system/operator/discovery").into(),
                ..request.clone()
            },
            supply::Fail::Conflict,
        ),
        (
            Install {
                slot: system_protocol::Mark::new(READY),
                ..request.clone()
            },
            supply::Fail::Conflict,
        ),
    ] {
        assert_eq!(
            finish(
                client
                    .begin::<supply::operations::Install>(changed, deadline())
                    .unwrap()
            ),
            Outcome::Failure(failure)
        );
    }
    let mut escalation = request.clone();
    escalation.rights.0.operations = Operations::Sign(SignRights::TRIGGER | SignRights::LISTEN);
    assert_eq!(
        finish(
            client
                .begin::<supply::operations::Install>(escalation, deadline())
                .unwrap()
        ),
        Outcome::Failure(supply::Fail::Denied)
    );
    let tombstone = success(finish(
        client
            .begin::<supply::operations::CancelInstall>(Installation { id: iid(99) }, deadline())
            .unwrap(),
    ));
    assert_eq!(tombstone.state, InstallState::Cancelled);
    assert_eq!(
        success(finish(
            client
                .begin::<supply::operations::Install>(
                    Install {
                        id: iid(99),
                        ..request.clone()
                    },
                    deadline()
                )
                .unwrap()
        )),
        tombstone
    );
    // Lost first application reply: recover the same ID, then settle its SDK owner.
    let mut lost = client
        .begin::<supply::operations::Install>(request.clone(), deadline())
        .unwrap();
    let accepted = inspect_install(&mut client, request.id);
    assert_eq!(accepted.target, prepared.task);
    while let Err(error) = lost.finish() {
        assert!(error.is_busy());
        tick(budget());
    }
    if loss {
        // No Start has been admitted. Required source loss must close this
        // Prepared attempt rather than resume a Task with a missing grant.
        success(finish(
            client
                .begin::<supply::operations::Withdraw>(
                    Registration { id: request.source },
                    deadline(),
                )
                .unwrap(),
        ));
        let cleared = settle(&mut client, request.id, InstallState::Released);
        assert_eq!(cleared.failure.0.unwrap().0, supply::Fail::SourceLost);
        while observe(&mut control, instance).phase != api::Phase::Retired {
            query_tick(budget());
        }
        let late = success(finish(
            control
                .begin::<op::Start>(
                    api::Change {
                        id: id(2),
                        instance,
                    },
                    deadline(),
                )
                .unwrap(),
        ));
        assert_eq!(receipt(&mut control, late).state, api::State::Failed);
        assert_eq!(
            success(finish(
                client
                    .begin::<supply::operations::Install>(request.clone(), deadline())
                    .unwrap()
            )),
            cleared
        );
        for value in [1, 2] {
            let id = RegistrationId::new(value).unwrap();
            success(finish(
                client
                    .begin::<supply::operations::Withdraw>(Registration { id }, deadline())
                    .unwrap(),
            ));
            while provider::status(&mut client, id).state != SourceState::Closed {
                query_tick(budget());
            }
        }
        for signal in [first, second] {
            sign::trigger(signal).unwrap();
            pulse(signal);
        }
        for offer in offers {
            retire(offer);
        }
        retire(first);
        retire(second);
        let _ = writeln!(
            Console,
            "PASS handoff install-loss=1 start-gate=1 cleanup=1"
        );
        return;
    }
    assert_eq!(
        finish(
            client
                .begin::<supply::operations::Install>(
                    Install {
                        slot: system_protocol::Mark::new(DEVICE + 4),
                        ..request.clone()
                    },
                    deadline()
                )
                .unwrap()
        ),
        Outcome::Failure(supply::Fail::Conflict)
    );
    assert_eq!(
        finish(
            client
                .begin::<supply::operations::Install>(
                    Install {
                        id: iid(50),
                        ..request.clone()
                    },
                    deadline()
                )
                .unwrap()
        ),
        Outcome::Failure(supply::Fail::Conflict)
    );
    let early = Install {
        id: iid(3),
        slot: system_protocol::Mark::new(DEVICE + 2),
        ..request.clone()
    };
    let mut lost = client
        .begin::<supply::operations::Install>(early.clone(), deadline())
        .unwrap();
    let cancelled = success(finish(
        client
            .begin::<supply::operations::CancelInstall>(Installation { id: early.id }, deadline())
            .unwrap(),
    ));
    assert!(matches!(
        cancelled.state,
        InstallState::Releasing | InstallState::Released
    ));
    while let Err(error) = lost.finish() {
        assert!(error.is_busy());
        tick(budget());
    }
    let cleared = settle(&mut client, early.id, InstallState::Released);
    assert_eq!(
        success(finish(
            client
                .begin::<supply::operations::Install>(early, deadline())
                .unwrap()
        )),
        cleared
    );
    let second_request = Install {
        id: iid(2),
        source: RegistrationId::new(2).unwrap(),
        slot: system_protocol::Mark::new(DEVICE + 1),
        ..request.clone()
    };
    success(finish(
        client
            .begin::<supply::operations::Install>(second_request.clone(), deadline())
            .unwrap(),
    ));
    let third_request = Install {
        id: iid(4),
        slot: system_protocol::Mark::new(DEVICE + 2),
        ..request.clone()
    };
    success(finish(
        client
            .begin::<supply::operations::Install>(third_request.clone(), deadline())
            .unwrap(),
    ));
    // Start can be accepted while installation is Pending; it must await all
    // actual deliveries before resuming the ordinary Driver Task.
    let start = success(finish(
        control
            .begin::<op::Start>(
                api::Change {
                    id: id(2),
                    instance,
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(receipt(&mut control, start).state, api::State::Completed);
    let observed = loop {
        let value = observe(&mut control, instance);
        if value.ready {
            break value;
        }
        query_tick(budget());
    };
    pulse(first);
    let granted = settle(&mut client, request.id, InstallState::Granted);
    assert_eq!(granted.instance.0, Some(instance));
    assert!(granted.delivery.0.is_some());
    assert_eq!(
        success(finish(
            client
                .begin::<supply::operations::Install>(request.clone(), deadline())
                .unwrap()
        )),
        granted
    );
    assert_eq!(
        inspect_install(&mut client, second_request.id).state,
        InstallState::Granted
    );
    assert_eq!(
        inspect_install(&mut client, third_request.id).state,
        InstallState::Granted
    );
    // A released slot is reused only under a new immutable ID. Late old
    // cancellation/replay must not select the new branch at that same mark.
    assert_eq!(
        success(finish(
            client
                .begin::<supply::operations::CancelInstall>(Installation { id: iid(3) }, deadline())
                .unwrap()
        )),
        cleared
    );

    assert_eq!(
        finish(
            client
                .begin::<supply::operations::Install>(
                    Install {
                        id: iid(51),
                        slot: system_protocol::Mark::new(DEVICE + 5),
                        ..request.clone()
                    },
                    deadline()
                )
                .unwrap()
        ),
        Outcome::Failure(supply::Fail::NotReady)
    );
    let publish = success(finish(
        control
            .begin::<op::Publish>(
                api::Publication {
                    id: id(3),
                    name: Name::new("driver").unwrap(),
                    expected: 0,
                    instance: Optional(Some(instance)),
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(publish.state, api::State::Completed);
    let mut discovery = discovery(license, context.clone());
    let resource = bind_resource(&mut discovery, instance);
    ping(&resource, &observed, license, context.clone(), false);
    success(finish(
        client
            .begin::<supply::operations::CancelInstall>(Installation { id: request.id }, deadline())
            .unwrap(),
    ));
    let released = settle(&mut client, request.id, InstallState::Released);
    assert!(released.failure.0.is_none());
    ping(&resource, &observed, license, context.clone(), true);
    assert_eq!(
        query(&resource, license, context.clone(), "second").reason,
        2
    );
    pulse(second);
    assert_eq!(
        provider::status(&mut client, request.source).state,
        SourceState::Active
    );
    success(finish(
        client
            .begin::<supply::operations::Withdraw>(
                Registration {
                    id: second_request.source,
                },
                deadline(),
            )
            .unwrap(),
    ));
    while provider::status(&mut client, second_request.source).state != SourceState::Closed {
        query_tick(budget());
    }
    let withdrawn = settle(&mut client, second_request.id, InstallState::Released);
    assert_eq!(withdrawn.failure.0.unwrap().0, supply::Fail::SourceLost);
    assert_eq!(
        query(&resource, license, context.clone(), "revoked-second").reason,
        4
    );
    assert_eq!(
        query(&resource, license, context.clone(), "third").reason,
        3
    );
    pulse(first);
    retire(resource.token.try_into().unwrap());
    let retired = success(finish(
        control
            .begin::<op::Retire>(
                api::Change {
                    id: id(4),
                    instance,
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(receipt(&mut control, retired).state, api::State::Completed);
    assert_eq!(observe(&mut control, instance).phase, api::Phase::Retired);
    assert_eq!(
        inspect_install(&mut client, third_request.id).state,
        InstallState::Released
    );
    assert_eq!(
        success(finish(
            client
                .begin::<supply::operations::CancelInstall>(
                    Installation { id: request.id },
                    deadline()
                )
                .unwrap()
        )),
        released
    );
    assert_eq!(
        success(finish(
            client
                .begin::<supply::operations::Install>(request.clone(), deadline())
                .unwrap()
        )),
        released
    );
    success(finish(
        client
            .begin::<supply::operations::Withdraw>(Registration { id: request.source }, deadline())
            .unwrap(),
    ));
    while provider::status(&mut client, request.source).state != SourceState::Closed {
        query_tick(budget());
    }
    for signal in [first, second] {
        sign::trigger(signal).unwrap();
        pulse(signal);
    }
    for offer in offers {
        retire(offer);
    }
    retire(first);
    retire(second);
    let _ = writeln!(
        Console,
        "PASS handoff post-prepared=1 target=1 replay=1 cancel=1 isolation=1 withdraw=1 retire=1"
    );
}

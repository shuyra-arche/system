//! External provider -> production System registry -> independent Driver.
//! No private System Source implementation participates in this scenario.
use super::*;
use supply::{Registration, RegistrationId, SourceState, SourceStatus};
fn status(client: &mut system_client::supply::Client, id: RegistrationId) -> SourceStatus {
    success(finish(
        client
            .begin::<supply::operations::InspectSource>(Registration { id }, deadline())
            .unwrap(),
    ))
}
fn register(client: &mut system_client::supply::Client, request: supply::Register) -> SourceStatus {
    loop {
        match finish(
            client
                .begin::<supply::operations::Register>(request.clone(), deadline())
                .unwrap(),
        ) {
            Outcome::Success(value) => return value,
            Outcome::Failure(supply::Fail::Pending) => query_tick(budget()),
            failure => panic!("registration {failure:?}"),
        }
    }
}
fn accord(source: PendId, target: AccordTarget, limits: Limits) -> PendId {
    let bytes = AccordRequest {
        src: source,
        target,
        mark: Mark::NONE,
        limits,
    }
    .to_bytes();
    loop {
        match pend::accord(VirtAddr::new(bytes.as_ptr().addr())) {
            Ok(token) => return token,
            Err(error) if error.source.is_busy() => tick(budget()),
            failure => panic!("provider delegation {failure:?}"),
        }
    }
}
/// Exercise the ordinary consumer adapter as well as construction installation.
/// A second call from the same live Driver must select the new registration.
pub(super) fn self_lease(
    client: &mut system_client::supply::Client,
    caller: TaskId,
    id: u64,
    pulse: bool,
) {
    let lease = supply::LeaseId::new(id).unwrap();
    let maximum = Limits {
        operations: Operations::Sign(SignRights::TRIGGER),
        modifiable: Modifiable::No,
        distributable: Distributable::No,
        scope: PendScope::Object,
    };
    let mut value = success(finish(
        client
            .begin::<supply::operations::Acquire>(
                supply::Acquire {
                    lease,
                    target: caller.into(),
                    binding: supply::Binding {
                        source: supply::Key {
                            name: "controller".into(),
                        },
                        resource: supply::Key {
                            name: "signal".into(),
                        },
                        slot: system_protocol::Mark::new(DEVICE + 1),
                        rights: Rights(maximum),
                    },
                },
                deadline(),
            )
            .unwrap(),
    ));
    while value.state != supply::State::Granted {
        query_tick(budget());
        value = success(finish(
            client
                .begin::<supply::operations::Inspect>(supply::Lease { lease }, deadline())
                .unwrap(),
        ));
    }
    let token = PendId::try_from(value.delivery.0.unwrap().token).unwrap();
    assert_eq!(inspect(token).limits, maximum);
    if pulse {
        sign::trigger(token).unwrap();
    }
    let mut value = success(finish(
        client
            .begin::<supply::operations::Release>(supply::Lease { lease }, deadline())
            .unwrap(),
    ));
    while value.state != supply::State::Released {
        query_tick(budget());
        value = success(finish(
            client
                .begin::<supply::operations::Inspect>(supply::Lease { lease }, deadline())
                .unwrap(),
        ));
    }
    let error = sign::trigger(token).expect_err("released consumer lease remained usable");
    assert!(!error.source.is_busy());
}
pub(super) fn run(license: PendId, context: Context) {
    let endpoint = find(|info| info.mark == Mark::new(7));
    let source_endpoint = find(|info| info.mark == Mark::new(8));
    let me = own();
    let UnitTarget::Task(caller) = me.target else {
        panic!("Task")
    };
    assert_eq!(endpoint.owner, source_endpoint.owner);
    assert_ne!(caller, source_endpoint.owner);
    let mut control = system_client::instance::Client::bind(
        endpoint.token,
        endpoint.owner,
        license,
        context.clone(),
    )
    .unwrap();
    let state_endpoint = find(|info| info.mark == Mark::new(10));
    let mut states = system_client::control::Client::bind(
        state_endpoint.token,
        state_endpoint.owner,
        license,
        context.clone(),
    )
    .unwrap();
    let named = control::Named {
        name: Name::new("controller").unwrap(),
    };
    assert_ne!(
        success(finish(
            states
                .begin::<control::operations::State>(named.clone(), deadline())
                .unwrap()
        ))
        .state,
        control::State::Ready
    );
    let mut provider = system_client::supply::Client::bind(
        source_endpoint.token,
        source_endpoint.owner,
        license,
        context.clone(),
    )
    .unwrap();
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
    let mut discovery = discovery(license, context.clone());
    let signal = sign::create(license).unwrap();
    let maximum = Limits {
        operations: Operations::Sign(SignRights::TRIGGER),
        modifiable: Modifiable::No,
        distributable: Distributable::No,
        scope: PendScope::Object,
    };
    let mut delegation = maximum;
    delegation.distributable = Distributable::Yes;
    let mut previous: Option<(api::InstanceId, operator::Resource, api::Observation)> = None;
    for round in 0..2 {
        let offer = accord(signal, AccordTarget::Caller, delegation);
        let remote = accord(offer, AccordTarget::Task(source_endpoint.owner), delegation);
        let registration = RegistrationId::new(round + 1).unwrap();
        let request = supply::Register {
            id: registration,
            source: supply::Key {
                name: "controller".into(),
            },
            resource: supply::Key {
                name: "signal".into(),
            },
            offered: remote.into(),
            maximum: Rights(maximum),
        };
        if round == 0 {
            let mut reserved = request.clone();
            reserved.source.name = "system".into();
            assert_eq!(
                finish(
                    provider
                        .begin::<supply::operations::Register>(reserved, deadline())
                        .unwrap()
                ),
                Outcome::Failure(supply::Fail::Conflict)
            );
            let mut spoof = request.clone();
            spoof.source.name = "driver".into();
            assert_eq!(
                finish(
                    provider
                        .begin::<supply::operations::Register>(spoof, deadline())
                        .unwrap()
                ),
                Outcome::Failure(supply::Fail::Denied)
            );
            let mut escalation = request.clone();
            escalation.maximum.0.operations =
                Operations::Sign(SignRights::TRIGGER | SignRights::LISTEN);
            assert_eq!(
                finish(
                    provider
                        .begin::<supply::operations::Register>(escalation, deadline())
                        .unwrap()
                ),
                Outcome::Failure(supply::Fail::Denied)
            );
        }
        assert_eq!(
            register(&mut provider, request.clone()).state,
            SourceState::Active
        );
        assert_eq!(
            register(&mut provider, request.clone()).state,
            SourceState::Active
        );
        let mut conflict = request.clone();
        conflict.resource.name = "different".into();
        assert_eq!(
            finish(
                provider
                    .begin::<supply::operations::Register>(conflict, deadline())
                    .unwrap()
            ),
            Outcome::Failure(supply::Fail::Conflict)
        );
        // Consumer cancellation has no authority over a registration with the
        // same numeric ID. No cross-Task Acquire bypass is introduced.
        let cancelled = success(finish(
            provider
                .begin::<supply::operations::Cancel>(
                    supply::Lease {
                        lease: supply::LeaseId::new(round + 1).unwrap(),
                    },
                    deadline(),
                )
                .unwrap(),
        ));
        assert_eq!(cancelled.state, supply::State::Closed);
        assert_eq!(
            status(&mut provider, registration).state,
            SourceState::Active
        );
        if let Some((old, resource, observed)) = previous.take() {
            // Same live Task, same source/resource, new root. The previous
            // Service source cache must not be reused for this Acquire.
            assert_eq!(
                query(&resource, license, context.clone(), "acquire").reason,
                2
            );
            let kernel_api::abi::call::sign::SignReply::Triggered { sequence } =
                sign::listen(signal, Wait::Poll).unwrap()
            else {
                panic!("missing new-generation consumer signal")
            };
            sign::consume(signal, sequence).unwrap();
            ping(&resource, &observed, license, context.clone(), true);
            retire(resource.token.try_into().unwrap());
            let retired = success(finish(
                control
                    .begin::<op::Retire>(
                        api::Change {
                            id: id(4),
                            instance: old,
                        },
                        deadline(),
                    )
                    .unwrap(),
            ));
            assert_eq!(receipt(&mut control, retired).state, api::State::Completed);
            assert_eq!(observe(&mut control, old).phase, api::Phase::Retired);
        }
        if round == 0 {
            sign::trigger(find(|info| info.mark == Mark::new(0xc0de)).token).unwrap();
            while success(finish(
                states
                    .begin::<control::operations::State>(named.clone(), deadline())
                    .unwrap(),
            ))
            .state
                != control::State::Ready
            {
                query_tick(budget());
            }
        }
        let base = 1 + round * 10;
        let created = success(finish(
            control
                .begin::<op::Create>(
                    api::Create {
                        id: id(base),
                        input: loader::Input::Unit(loader::UnitInput {
                            name: Name::new("driver").unwrap(),
                            stack: 512 * 1024,
                            arguments: loader::Arguments { values: vec![15] },
                        }),
                        subject: subject.clone(),
                    },
                    deadline(),
                )
                .unwrap(),
        ));
        let target = created.instance.0.unwrap();
        assert_eq!(receipt(&mut control, created).state, api::State::Completed);
        let prepared = observe(&mut control, target);
        assert_eq!(prepared.phase, api::Phase::Prepared);
        assert_ne!(prepared.team.get(), me.team.get() as u64);
        assert_ne!(prepared.task.get(), caller.get() as u64);
        let started = success(finish(
            control
                .begin::<op::Start>(
                    api::Change {
                        id: id(base + 1),
                        instance: target,
                    },
                    deadline(),
                )
                .unwrap(),
        ));
        assert_eq!(receipt(&mut control, started).state, api::State::Completed);
        let observed = loop {
            let value = observe(&mut control, target);
            if value.ready {
                break value;
            }
            query_tick(budget());
        };
        let kernel_api::abi::call::sign::SignReply::Triggered { sequence } =
            sign::listen(signal, Wait::Poll).unwrap()
        else {
            panic!("missing installed signal")
        };
        sign::consume(signal, sequence).unwrap();
        let expected = if round == 0 {
            0
        } else {
            let cleared = success(finish(
                discovery
                    .begin::<operator::operations::ResolveRoot>(
                        operator::Resolve {
                            mark: Mark::of("driver").into(),
                            name: Name::new("driver").unwrap(),
                        },
                        deadline(),
                    )
                    .unwrap(),
            ));
            assert!(cleared.instance.0.is_none() && cleared.pane.0.is_none());
            assert_eq!(
                cleared.revision, 2,
                "Closing clears the previous association"
            );
            cleared.revision
        };
        let published = success(finish(
            control
                .begin::<op::Publish>(
                    api::Publication {
                        id: id(base + 2),
                        name: Name::new("driver").unwrap(),
                        expected,
                        instance: Optional(Some(target)),
                    },
                    deadline(),
                )
                .unwrap(),
        ));
        assert_eq!(published.state, api::State::Completed);
        let resource = bind_resource(&mut discovery, target);
        ping(&resource, &observed, license, context.clone(), false);
        // The existing consumer contract still rejects an arbitrary other Task.
        let unauthorized = supply::Acquire {
            lease: supply::LeaseId::new(50 + round).unwrap(),
            target: observed.task,
            binding: supply::Binding {
                source: request.source.clone(),
                resource: request.resource.clone(),
                slot: system_protocol::Mark::new(DEVICE),
                rights: Rights(maximum),
            },
        };
        assert_eq!(
            finish(
                provider
                    .begin::<supply::operations::Acquire>(unauthorized, deadline())
                    .unwrap()
            ),
            Outcome::Failure(supply::Fail::Denied)
        );
        if round == 0 {
            let closing = success(finish(
                provider
                    .begin::<supply::operations::Withdraw>(
                        Registration { id: registration },
                        deadline(),
                    )
                    .unwrap(),
            ));
            assert!(matches!(
                closing.state,
                SourceState::Closing | SourceState::Closed
            ));
        } else {
            // The provider can revoke its own ancestor independently. System
            // must observe and settle its missing imported branch, not revive it.
            retire(offer);
        }
        while status(&mut provider, registration).state != SourceState::Closed {
            query_tick(budget());
        }
        assert_eq!(
            register(&mut provider, request.clone()).state,
            SourceState::Closed
        );
        ping(&resource, &observed, license, context.clone(), true);
        // Original provider authority survives withdrawal of the offered branch.
        sign::trigger(signal).unwrap();
        let kernel_api::abi::call::sign::SignReply::Triggered { sequence } =
            sign::listen(signal, Wait::Poll).unwrap()
        else {
            panic!("original resource lost")
        };
        sign::consume(signal, sequence).unwrap();
        if round == 0 {
            retire(offer);
            previous = Some((target, resource, observed));
        } else {
            retire(resource.token.try_into().unwrap());
            let retired = success(finish(
                control
                    .begin::<op::Retire>(
                        api::Change {
                            id: id(base + 3),
                            instance: target,
                        },
                        deadline(),
                    )
                    .unwrap(),
            ));
            assert_eq!(receipt(&mut control, retired).state, api::State::Completed);
            assert_eq!(observe(&mut control, target).phase, api::Phase::Retired);
        }
    }
    let cancelled = RegistrationId::new(99).unwrap();
    assert_eq!(
        success(finish(
            provider
                .begin::<supply::operations::Withdraw>(Registration { id: cancelled }, deadline())
                .unwrap()
        ))
        .state,
        SourceState::Cancelled
    );
    let offer = accord(signal, AccordTarget::Caller, delegation);
    let remote = accord(offer, AccordTarget::Task(source_endpoint.owner), delegation);
    assert_eq!(
        register(
            &mut provider,
            supply::Register {
                id: cancelled,
                source: supply::Key {
                    name: "controller".into()
                },
                resource: supply::Key {
                    name: "signal".into()
                },
                offered: remote.into(),
                maximum: Rights(maximum)
            }
        )
        .state,
        SourceState::Cancelled
    );
    // Cancelled never accepted the input. Retire the provider-owned offering
    // branch explicitly; Closed is reserved for an accepted branch's completion.
    retire(offer);
    retire(signal);
    let _ = writeln!(
        Console,
        "PASS handoff provider=1 register=1 installed=1 replay=1 revoke=1 ancestor=1 rebind=1"
    );
}

use super::{
    Effects, Poll,
    production_gate::Gate,
    reset,
    retirement::anchor,
    system::app::{namespace::Layout, services::Services},
};
use kernel_api::model::identity::{PendId, TaskId};
use std::collections::VecDeque;
use unit_contract::document::Retirement;

fn fixture(effects: Effects) -> (Gate, Services, Layout) {
    reset(effects);
    let declarations = [
        Retirement {
            provider: "sensor",
            namespace: 1,
            face: "retire",
        },
        Retirement {
            provider: "storage",
            namespace: 2,
            face: "retire",
        },
    ];
    (
        Gate::new(native_task(9), &declarations),
        Services {
            license: PendId::try_from(system_protocol::PendId::new(11)).unwrap(),
            context: super::Context,
        },
        Layout,
    )
}

fn effects() -> Effects {
    Effects::default()
}

fn native_task(value: u64) -> TaskId {
    TaskId::try_from(system_protocol::TaskId::new(value)).unwrap()
}

fn single_fixture(effects: Effects) -> (Gate, Services, Layout) {
    reset(effects);
    let declarations = [Retirement {
        provider: "sensor",
        namespace: 1,
        face: "retire",
    }];
    (
        Gate::new(native_task(9), &declarations),
        Services {
            license: PendId::try_from(system_protocol::PendId::new(11)).unwrap(),
            context: super::Context,
        },
        Layout,
    )
}

fn advance(gate: &mut Gate, services: &Services, layout: &Layout) -> bool {
    gate.advance(services, layout, |provider, namespace| {
        assert!(matches!(
            (provider, namespace),
            ("sensor", 1) | ("storage", 2)
        ));
        Some(anchor())
    })
}

#[test]
fn pending_provider_receipt_and_cleanup_block_stop_until_settled() {
    let mut scripted = effects();
    scripted.polls = VecDeque::from([
        Poll::Pending,
        Poll::Unsettled(9),
        Poll::Failure,
        Poll::Settled(9),
        Poll::Settled(9),
    ]);
    let (mut gate, services, layout) = fixture(scripted);

    assert!(!advance(&mut gate, &services, &layout)); // Start provider request.
    assert!(!advance(&mut gate, &services, &layout)); // Pending is not settlement.
    assert!(!advance(&mut gate, &services, &layout)); // Unsettled reply is not proof.
    assert!(!advance(&mut gate, &services, &layout)); // Retry same obligation after unsettled reply.
    assert!(!advance(&mut gate, &services, &layout)); // Failed provider response is not settlement.
    assert!(!advance(&mut gate, &services, &layout)); // Retry the same obligation again.
    assert!(!advance(&mut gate, &services, &layout)); // Settlement reply still needs cleanup.
    assert!(!advance(&mut gate, &services, &layout)); // Release first root; start second.
    assert!(!advance(&mut gate, &services, &layout)); // Second provider settles.
    assert!(advance(&mut gate, &services, &layout)); // Cleanup/release, then Stop is safe.

    super::EFFECTS.with(|slot| {
        let calls = &slot.borrow().calls;
        assert_eq!(
            calls,
            &[
                "begin:9", "poll", "poll", "cleanup", "begin:9", "poll", "cleanup", "begin:9",
                "poll", "cleanup", "release", "begin:9", "poll", "cleanup", "release"
            ]
        );
    });
}

#[test]
fn failed_begin_cleanup_and_root_release_remain_retryable() {
    let mut scripted = effects();
    scripted.begins = VecDeque::from([Err(super::TransportError), Ok(()), Ok(())]);
    scripted.polls = VecDeque::from([Poll::Settled(8), Poll::Settled(9), Poll::Settled(9)]);
    scripted.cleanups = VecDeque::from([Err(super::TransportError), Ok(()), Ok(()), Ok(())]);
    scripted.releases = VecDeque::from([false, true, true]);
    let (mut gate, services, layout) = fixture(scripted);

    assert!(!advance(&mut gate, &services, &layout)); // Begin failure keeps cleanup.
    assert!(!advance(&mut gate, &services, &layout)); // Failed cleanup cannot retry.
    assert!(!advance(&mut gate, &services, &layout)); // Retry same target and begin.
    assert!(!advance(&mut gate, &services, &layout)); // Wrong-task receipt is not settlement.
    assert!(!advance(&mut gate, &services, &layout)); // Retry same root after cleanup.
    assert!(!advance(&mut gate, &services, &layout)); // Settled receipt still needs cleanup.
    assert!(!advance(&mut gate, &services, &layout)); // Failed root release retains it.
    assert!(!advance(&mut gate, &services, &layout)); // Release retry then next begin.
    assert!(!advance(&mut gate, &services, &layout)); // Settle second participant.
    assert!(advance(&mut gate, &services, &layout)); // Every root released.

    super::EFFECTS.with(|slot| {
        let calls = &slot.borrow().calls;
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.as_str() == "release")
                .count(),
            3
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.as_str() == "cleanup")
                .count(),
            5
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.as_str() == "begin:9")
                .count(),
            4
        );
    });
}

#[test]
fn missing_provider_and_transport_poll_failure_keep_the_gate_closed() {
    let mut scripted = effects();
    scripted.polls = VecDeque::from([Poll::TransportFailure, Poll::Settled(9)]);
    let (mut gate, services, layout) = single_fixture(scripted);

    assert!(!gate.advance(&services, &layout, |_, _| None));
    super::EFFECTS.with(|slot| assert!(slot.borrow().calls.is_empty()));

    assert!(!advance(&mut gate, &services, &layout)); // Begin provider request.
    assert!(!advance(&mut gate, &services, &layout)); // Lost reply retains cleanup.
    assert!(!advance(&mut gate, &services, &layout)); // Retry same declared provider.
    assert!(!advance(&mut gate, &services, &layout)); // Settlement still needs cleanup.
    assert!(advance(&mut gate, &services, &layout)); // Only settled, clean and released.

    super::EFFECTS.with(|slot| {
        let calls = &slot.borrow().calls;
        assert_eq!(
            calls,
            &[
                "begin:9", "poll", "cleanup", "begin:9", "poll", "cleanup", "release"
            ]
        );
    });
}

#[test]
fn failed_resource_discovery_keeps_the_provider_requirement_for_retry() {
    let mut scripted = effects();
    scripted.resource_failures = VecDeque::from([true, false]);
    scripted.polls = VecDeque::from([Poll::Settled(9)]);
    let (mut gate, services, layout) = single_fixture(scripted);

    assert!(!gate.advance(&services, &layout, |_, _| Some(anchor())));
    assert!(!advance(&mut gate, &services, &layout)); // Retry lookup then begin.
    assert!(!advance(&mut gate, &services, &layout)); // Settled receipt needs cleanup.
    assert!(advance(&mut gate, &services, &layout)); // Requirement releases only now.

    super::EFFECTS.with(|slot| {
        let calls = &slot.borrow().calls;
        assert_eq!(calls, &["begin:9", "poll", "cleanup", "release"]);
    });
}

#[test]
fn another_instances_receipt_cannot_discharge_this_instance() {
    reset(Effects {
        polls: VecDeque::from([Poll::Settled(10), Poll::Settled(10), Poll::Settled(9)]),
        ..effects()
    });
    let declaration = [Retirement {
        provider: "sensor",
        namespace: 1,
        face: "retire",
    }];
    let mut first = Gate::new(native_task(9), &declaration);
    let mut second = Gate::new(native_task(10), &declaration);
    let services = Services {
        license: PendId::try_from(system_protocol::PendId::new(11)).unwrap(),
        context: super::Context,
    };
    let layout = Layout;
    let mut lookup = |_: &str, _: usize| Some(anchor());

    assert!(!first.advance(&services, &layout, &mut lookup));
    assert!(!second.advance(&services, &layout, &mut lookup));
    assert!(!first.advance(&services, &layout, &mut lookup));
    assert!(!second.advance(&services, &layout, &mut lookup));
    assert!(!first.advance(&services, &layout, &mut lookup));
    assert!(second.advance(&services, &layout, &mut lookup));
    assert!(!first.advance(&services, &layout, &mut lookup));
    assert!(first.advance(&services, &layout, &mut lookup));

    super::EFFECTS.with(|slot| {
        let calls = &slot.borrow().calls;
        assert_eq!(
            calls,
            &[
                "begin:9", "begin:10", "poll", "poll", "cleanup", "begin:9", "cleanup", "release",
                "poll", "cleanup", "release"
            ]
        );
    });
}

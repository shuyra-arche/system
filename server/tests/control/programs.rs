use super::program::Program;
use crate::{declaration::Unit, shutdown};
use alloc::sync::Arc;
use system_protocol::control::instance::Sequence;
use unit_contract::document as d;
fn program(ending: d::Ending, guarded: bool) -> Program {
    Program::new(Arc::new(
        Unit::compile(d::v2::Unit {
            schema: 2,
            id: "worker",
            loader: d::Loader {
                image: "worker",
                ..Default::default()
            },
            control: d::Control {
                ending,
                guarded,
                groups: vec!["workers"],
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap(),
    ))
}
#[test]
fn stopping_preserves_selection_and_does_not_invent_an_attempt() {
    let mut program = program(d::Ending::Transient, false);
    program.stop();
    assert_eq!(program.current(), None);
    assert!(!program.requested());
    assert!(program.eligible(|_| panic!("unselected Program has no physical attempt")));
    let first = Sequence::new(1).unwrap();
    program.select(first);
    assert_eq!(program.current(), Some(first));
    assert!(program.requested());
    program.stop();
    program.stop();
    assert_eq!(program.current(), Some(first));
    assert!(!program.requested());
    assert!(!program.eligible(|id| {
        assert_eq!(id, first);
        false
    }));
    assert!(program.eligible(|id| {
        assert_eq!(id, first);
        true
    }));
    let next = Sequence::new(2).unwrap();
    program.select(next);
    assert_eq!(program.current(), Some(next));
    assert!(program.requested());
}
#[test]
fn transient_completion_still_requires_physical_retirement() {
    let mut program = program(d::Ending::Transient, false);
    program.select(Sequence::new(1).unwrap());
    assert!(!program.eligible(|_| false));
    assert!(program.eligible(|_| true));
    assert!(program.requested());
}
#[test]
fn resident_completion_requires_stopped_intent_and_physical_retirement() {
    let mut program = program(d::Ending::Resident, false);
    program.select(Sequence::new(1).unwrap());
    assert!(!program.eligible(|_| true));
    program.stop();
    assert!(!program.eligible(|_| false));
    assert!(program.eligible(|_| true));
}
#[test]
fn supervision_requires_matching_group_demand_and_unguarded_policy() {
    for guarded in [false, true] {
        let mut program = program(d::Ending::Transient, guarded);
        assert!(!program.supervised_by("workers"));
        program.select(Sequence::new(1).unwrap());
        assert!(!program.supervised_by("other"));
        assert_eq!(program.supervised_by("workers"), !guarded);
        program.stop();
        assert!(!program.supervised_by("workers"));
    }
}

fn selected(name: &'static str, id: u64, control: d::Control<&'static str>) -> Program {
    let mut program = Program::new(Arc::new(
        Unit::compile(d::v2::Unit {
            schema: 2,
            id: name,
            loader: d::Loader {
                image: name,
                ..Default::default()
            },
            control,
            ..Default::default()
        })
        .unwrap(),
    ));
    program.select(Sequence::new(id).unwrap());
    program
}

#[test]
fn shutdown_waits_for_live_required_and_retirement_dependents() {
    let provider = selected("provider", 1, d::Control::default());
    let consumer = selected(
        "consumer",
        2,
        d::Control {
            requires: vec!["provider"],
            ..Default::default()
        },
    );
    let programs = [provider, consumer];
    let live = shutdown::stoppable(&programs, |_| false);
    assert!(!live.contains(&Sequence::new(1).unwrap()));
    assert!(live.contains(&Sequence::new(2).unwrap()));

    let retired = shutdown::stoppable(&programs, |id| id == Sequence::new(2).unwrap());
    assert!(retired.contains(&Sequence::new(1).unwrap()));

    let unselected_dependent = Program::new(Arc::new(
        Unit::compile(d::v2::Unit {
            schema: 2,
            id: "unselected-consumer",
            loader: d::Loader {
                image: "unselected-consumer",
                ..Default::default()
            },
            control: d::Control {
                requires: vec!["provider"],
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap(),
    ));
    let programs = [selected("provider", 1, d::Control::default()), unselected_dependent];
    let without_live_consumer = shutdown::stoppable(&programs, |_| false);
    assert!(without_live_consumer.contains(&Sequence::new(1).unwrap()));

    let retirement_consumer = selected(
        "retirement-consumer",
        3,
        d::Control {
            retirement: vec![d::Retirement {
                provider: "provider",
                namespace: 0,
                face: "service",
            }],
            ..Default::default()
        },
    );
    let programs = [
        selected("provider", 1, d::Control::default()),
        retirement_consumer,
    ];
    let live = shutdown::stoppable(&programs, |_| false);
    assert!(!live.contains(&Sequence::new(1).unwrap()));
    assert!(live.contains(&Sequence::new(3).unwrap()));
}

#[test]
fn shutdown_releases_dependency_chain_from_leaf_toward_provider() {
    let root = selected("root", 1, d::Control::default());
    let middle = selected(
        "middle",
        2,
        d::Control {
            requires: vec!["root"],
            ..Default::default()
        },
    );
    let leaf = selected(
        "leaf",
        3,
        d::Control {
            requires: vec!["middle"],
            ..Default::default()
        },
    );
    let programs = [root, middle, leaf];
    let first = shutdown::stoppable(&programs, |_| false);
    assert_eq!(first, vec![Sequence::new(3).unwrap()]);

    let second = shutdown::stoppable(&programs, |id| id == Sequence::new(3).unwrap());
    assert!(second.contains(&Sequence::new(2).unwrap()));
    assert!(!second.contains(&Sequence::new(1).unwrap()));

    let final_round = shutdown::stoppable(&programs, |id| {
        id == Sequence::new(3).unwrap() || id == Sequence::new(2).unwrap()
    });
    assert!(final_round.contains(&Sequence::new(1).unwrap()));
}

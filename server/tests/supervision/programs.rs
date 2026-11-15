use super::{declaration::Unit, program::Program};
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

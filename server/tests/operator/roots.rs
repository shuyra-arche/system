// Pure registry policy. Native foreign-Team proof belongs to the System guest.
use super::unit::Registry;
use system_protocol::{
    EntryId, Mark, Name, TaskId, TeamId,
    control::instance::{InstanceId, Sequence},
    operator::{self as op, unit as api},
    value::Optional,
};
fn registration(sequence: u64, owner: u64) -> api::Registration {
    api::Registration {
        instance: InstanceId {
            system: TaskId::new(1),
            sequence: Sequence::new(sequence).unwrap(),
        },
        owner: TaskId::new(owner),
        task: TaskId::new(sequence + 10),
        team: TeamId::new(sequence + 20),
        pane: op::Binding {
            entry: op::Entry {
                operator: TaskId::new(2),
                target: op::EntryTarget::Node(EntryId::new(sequence)),
            },
            revision: sequence,
        },
    }
}
fn change(root: &api::Root, phase: api::Phase) -> api::Advance {
    api::Advance {
        instance: root.registration.instance,
        expected: root.revision,
        phase,
    }
}
fn association(id: u64, expected: u64, instance: InstanceId) -> api::Associate {
    api::Associate {
        id: api::AssociationId::new(id).unwrap(),
        mark: Mark::new(kernel_api::model::identity::Mark::of("driver").get()),
        name: Name::new("driver").unwrap(),
        expected,
        instance: Optional(Some(instance)),
    }
}
#[test]
fn root_registration_and_transitions_replay_the_original_commit() {
    let mut roots = Registry::default();
    let request = registration(1, 5);
    let preparing = roots.register(request.clone()).unwrap();
    assert_eq!(roots.records().len(), 1);
    assert!(roots.records()[0].grants().is_empty());
    assert_eq!(roots.records()[0].root(), &preparing);
    roots.prepare_grant(0).unwrap();
    roots.link_grant(
        0,
        crate::authority::GrantKey::Issued(core::num::NonZeroU64::new(1).unwrap()),
    );
    assert_eq!(roots.records()[0].grants().len(), 1);
    assert_eq!(
        roots.advance(change(&preparing, api::Phase::Retired)),
        Err(op::Fail::Bad)
    );
    let transition = change(&preparing, api::Phase::Active);
    let active = roots.advance(transition.clone()).unwrap();
    let closing = roots.advance(change(&active, api::Phase::Closing)).unwrap();
    assert_eq!(roots.prepare_grant(0), Err(op::Fail::Denied));
    assert_eq!(roots.advance(transition.clone()), Ok(active));
    assert_eq!(
        roots.replay(&transition),
        Some(Ok(roots.advance(transition).unwrap()))
    );
    assert_eq!(roots.register(request.clone()), Ok(preparing));
    let mut changed = request;
    changed.owner = TaskId::new(6);
    assert_eq!(roots.register(changed), Err(op::Fail::Conflict));
    assert_eq!(roots.get(closing.registration.instance).unwrap(), &closing);
    assert_eq!(
        roots.advance(change(&closing, api::Phase::Active)),
        Err(op::Fail::Bad)
    );
    let retiring = roots
        .advance(change(&closing, api::Phase::Retiring))
        .unwrap();
    let retired = roots
        .advance(change(&retiring, api::Phase::Retired))
        .unwrap();
    assert_eq!(
        roots.advance(change(&retired, api::Phase::Active)),
        Err(op::Fail::Bad)
    );
}
#[test]
fn ready_root_compare_replace_retains_history_and_closing_clears_discovery() {
    let mut roots = Registry::default();
    let a = roots.register(registration(1, 5)).unwrap();
    let b = roots.register(registration(2, 5)).unwrap();
    let first = association(1, 0, a.registration.instance);
    assert_eq!(roots.associate(first.clone()), Err(op::Fail::Denied));
    let a = roots.advance(change(&a, api::Phase::Active)).unwrap();
    let b = roots.advance(change(&b, api::Phase::Active)).unwrap();
    let published = roots.associate(first.clone()).unwrap();
    assert_eq!(published.revision, 1);
    assert_eq!(
        roots.associate(association(2, 0, b.registration.instance)),
        Err(op::Fail::Conflict)
    );
    let replacement = roots
        .associate(association(2, 1, b.registration.instance))
        .unwrap();
    assert_eq!(replacement.revision, 2);
    roots.advance(change(&a, api::Phase::Closing)).unwrap();
    assert_eq!(roots.resolve(first.mark, &first.name).unwrap(), replacement);
    assert_eq!(roots.associate(first.clone()), Ok(published));
    assert_eq!(
        roots.associate(association(1, 2, b.registration.instance)),
        Err(op::Fail::Conflict)
    );
    roots.advance(change(&b, api::Phase::Closing)).unwrap();
    let unbound = roots.resolve(first.mark, &first.name).unwrap();
    assert_eq!(unbound.instance.0, None);
    assert_eq!(unbound.owner, TaskId::new(5));
    assert_eq!(unbound.revision, 3);
}
#[test]
fn a_different_owner_cannot_take_a_live_or_tombstoned_name() {
    let mut roots = Registry::default();
    let a = roots.register(registration(1, 5)).unwrap();
    let b = roots.register(registration(2, 6)).unwrap();
    let a = roots.advance(change(&a, api::Phase::Active)).unwrap();
    let b = roots.advance(change(&b, api::Phase::Active)).unwrap();
    roots
        .associate(association(1, 0, a.registration.instance))
        .unwrap();
    assert_eq!(
        roots.associate(association(2, 1, b.registration.instance)),
        Err(op::Fail::Denied)
    );
    roots.advance(change(&a, api::Phase::Closing)).unwrap();
    assert_eq!(
        roots.associate(association(2, 2, b.registration.instance)),
        Err(op::Fail::Denied)
    );
}

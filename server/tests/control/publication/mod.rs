#[path = "../../../src/system/control/publication.rs"]
mod owner;

use system_protocol::{
    EntryId, TaskId,
    identity::{self, CoalitionId, IdentityObject, OperatorEntryRef, PrincipalId},
    operator::{Binding, Entry, EntryTarget},
};

fn principal() -> PrincipalId {
    PrincipalId {
        authority: TaskId::new(3),
        slot: 19,
    }
}

fn binding(operator: u64, target: EntryTarget, revision: u64) -> Binding {
    Binding {
        entry: Entry {
            operator: TaskId::new(operator),
            target,
        },
        revision,
    }
}

#[test]
fn declared_alias_and_current_subject_are_required_for_export() {
    assert!(owner::declares_alias(&["account", "billing"], "account"));
    assert!(!owner::declares_alias(&["account", "billing"], "other"));

    let principal = principal();
    assert_eq!(
        owner::principal(&IdentityObject::Principal(principal)),
        Some(principal)
    );
    assert_eq!(
        owner::principal(&IdentityObject::Coalition(CoalitionId {
            authority: TaskId::new(3),
            slot: 4,
        })),
        None
    );

    let subject = identity::Subject::new(principal, &[]).unwrap();
    assert!(owner::matches_subject(principal, &subject));
    assert!(!owner::matches_subject(
        PrincipalId {
            authority: TaskId::new(3),
            slot: 20,
        },
        &subject
    ));
}

#[test]
fn export_requires_the_exact_nonroot_operator_entry_reference() {
    let entry = OperatorEntryRef {
        operator: TaskId::new(21),
        root: false,
        id: EntryId::new(0),
        revision: 47,
    };
    let resource = binding(21, EntryTarget::Node(EntryId::new(0)), 47);
    assert!(owner::matches_entry(&resource, &entry));

    for changed in [
        OperatorEntryRef {
            operator: TaskId::new(22),
            ..entry.clone()
        },
        OperatorEntryRef {
            id: EntryId::new(1),
            ..entry.clone()
        },
        OperatorEntryRef {
            revision: 48,
            ..entry.clone()
        },
        OperatorEntryRef {
            root: true,
            ..entry.clone()
        },
    ] {
        assert!(!owner::matches_entry(&resource, &changed));
    }
    assert!(!owner::matches_entry(
        &binding(21, EntryTarget::Root, 47),
        &entry
    ));
}

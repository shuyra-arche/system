use super::installs::{Intent, Journal};
use alloc::sync::Arc;
use kernel_api::model::{
    identity::TaskId,
    modifiable::Modifiable,
    pend::{Distributable, Limits, Operations, PendScope},
    sign::SignRights,
};
use system_protocol::{
    Mark,
    control::instance::{InstanceId, Sequence},
    supply::*,
    value::Rights,
};
fn request(value: u64) -> Install {
    Install {
        id: InstallId::new(value).unwrap(),
        source: RegistrationId::new(1).unwrap(),
        instance: InstanceId {
            system: system_protocol::TaskId::new(3),
            sequence: Sequence::new(1).unwrap(),
        },
        target: system_protocol::TaskId::new(5),
        slot: Mark::new(42),
        rights: Rights(Limits {
            operations: Operations::Sign(SignRights::TRIGGER),
            modifiable: Modifiable::No,
            distributable: Distributable::No,
            scope: PendScope::Object,
        }),
    }
}
#[test]
fn accepted_install_retains_the_same_intent_and_caller_scope() {
    let mut journal = Journal::default();
    let caller = TaskId::new(4);
    let original = Arc::new(request(1));
    assert!(journal.replay(caller, &original).unwrap().is_none());
    journal.reserve().unwrap();
    journal.insert(caller, original.clone());
    let Some(Intent::Accepted(replayed)) = journal.replay(caller, &original).unwrap() else {
        panic!("accepted")
    };
    assert!(Arc::ptr_eq(&original, &replayed));
    assert!(journal.find(TaskId::new(6), original.id).is_none());
    for changed in [
        Install {
            target: system_protocol::TaskId::new(6),
            ..(*original).clone()
        },
        Install {
            source: RegistrationId::new(2).unwrap(),
            ..(*original).clone()
        },
        Install {
            slot: Mark::new(43),
            ..(*original).clone()
        },
    ] {
        assert!(matches!(
            journal.replay(caller, &changed),
            Err(Fail::Conflict)
        ));
    }
    // Cancel selects the retained identity; it does not rewrite its request.
    assert!(
        matches!(journal.cancel(caller, original.id).unwrap(), Intent::Accepted(old) if Arc::ptr_eq(&old, &original))
    );
}
#[test]
fn cancellation_tombstone_prevents_late_install_and_is_caller_local() {
    let mut journal = Journal::default();
    let caller = TaskId::new(4);
    let value = request(1);
    for _ in 0..2 {
        assert!(matches!(
            journal.cancel(caller, value.id),
            Ok(Intent::Cancelled(_))
        ));
    }
    assert!(matches!(
        journal.replay(caller, &value),
        Ok(Some(Intent::Cancelled(_)))
    ));
    assert!(journal.replay(TaskId::new(5), &value).unwrap().is_none());
}
#[test]
fn saturation_preserves_accepted_and_cancelled_history() {
    let mut journal = Journal::default();
    let caller = TaskId::new(4);
    journal.reserve().unwrap();
    journal.insert(caller, Arc::new(request(1)));
    for value in 2..=MAX_INSTALLATIONS as u64 {
        journal
            .cancel(caller, InstallId::new(value).unwrap())
            .unwrap();
    }
    assert_eq!(journal.reserve(), Err(Fail::Full));
    assert!(matches!(
        journal.cancel(caller, InstallId::new(257).unwrap()),
        Err(Fail::Full)
    ));
    assert!(matches!(
        journal.replay(caller, &request(1)),
        Ok(Some(Intent::Accepted(_)))
    ));
    assert!(matches!(
        journal.cancel(caller, InstallId::new(2).unwrap()),
        Ok(Intent::Cancelled(_))
    ));
}

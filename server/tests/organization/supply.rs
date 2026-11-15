use super::providers::Providers;
use kernel_api::model::{
    identity::{Mark, PendId, TaskId},
    modifiable::Modifiable,
    pend::{Distributable, Limits, Operations, PendInfo, PendScope, PendStatus, Target},
    sign::SignRights,
};
use system_protocol::{
    supply::{Fail, Key, Register, RegistrationId, SourceState},
    value::Rights,
};
fn caller() -> TaskId {
    TaskId::new(7)
}
fn request(id: u64) -> Register {
    Register {
        id: RegistrationId::new(id).unwrap(),
        source: Key { name: "hub".into() },
        resource: Key {
            name: "signal".into(),
        },
        offered: system_protocol::PendId::new(100 + id),
        maximum: Rights(Limits {
            operations: Operations::Sign(SignRights::TRIGGER),
            modifiable: Modifiable::No,
            distributable: Distributable::No,
            scope: PendScope::Object,
        }),
    }
}
fn info(token: PendId) -> PendInfo {
    let mut limits = request(1).maximum.0;
    limits.distributable = Distributable::Yes;
    PendInfo {
        token,
        target: Target::of([6, 40]).unwrap(),
        limits,
        owner: caller(),
        lender: caller(),
        mark: Mark::NONE,
        status: PendStatus::Active,
    }
}
fn register(p: &mut Providers, r: Register) -> Result<system_protocol::supply::SourceStatus, Fail> {
    p.register(caller(), Some("hub"), r, |token| Ok(info(token)))
}
#[test]
fn registration_requires_live_unit_name_and_actual_lender_at_admission() {
    let mut p = Providers::default();
    let r = request(1);
    assert_eq!(
        p.register(caller(), Some("other"), r.clone(), |_| panic!(
            "unauthorized"
        )),
        Err(Fail::Denied)
    );
    assert_eq!(
        p.register(caller(), None, r.clone(), |_| panic!("unauthorized")),
        Err(Fail::Denied)
    );
    assert_eq!(
        p.register(caller(), Some("hub"), r.clone(), |token| {
            let mut v = info(token);
            v.lender = TaskId::new(8);
            Ok(v)
        }),
        Err(Fail::Denied)
    );
    assert_eq!(p.inspect(caller(), r.id), Err(Fail::Unknown));
    assert_eq!(register(&mut p, r).unwrap().state, SourceState::Active);
}
#[test]
fn suspended_grants_and_rights_escalation_never_commit_registration() {
    let mut p = Providers::default();
    let r = request(1);
    assert_eq!(
        p.register(caller(), Some("hub"), r.clone(), |token| {
            let mut v = info(token);
            v.status = PendStatus::Suspended;
            Ok(v)
        }),
        Err(Fail::Denied)
    );
    assert_eq!(
        p.register(caller(), Some("hub"), r.clone(), |token| {
            let mut v = info(token);
            v.limits.distributable = Distributable::No;
            Ok(v)
        }),
        Err(Fail::Denied)
    );
    let mut escalation = r.clone();
    escalation.maximum.0.operations = Operations::Sign(SignRights::TRIGGER | SignRights::LISTEN);
    assert_eq!(register(&mut p, escalation), Err(Fail::Denied));
    assert!(p.find("hub", "signal").is_none());
    assert_eq!(register(&mut p, r).unwrap().state, SourceState::Active);
}
#[test]
fn replay_keeps_original_intent_and_foreign_caller_cannot_withdraw() {
    let mut p = Providers::default();
    let r = request(1);
    register(&mut p, r.clone()).unwrap();
    assert_eq!(
        p.register(caller(), None, r.clone(), |_| panic!(
            "replay must not import again"
        ))
        .unwrap()
        .state,
        SourceState::Active
    );
    let mut changed = r.clone();
    changed.resource.name = "other".into();
    assert_eq!(register(&mut p, changed), Err(Fail::Conflict));
    assert_eq!(
        p.withdraw(TaskId::new(8), r.id).unwrap().state,
        SourceState::Cancelled
    );
    assert_eq!(
        p.inspect(TaskId::new(8), r.id).unwrap().state,
        SourceState::Cancelled
    );
    assert!(p.find("hub", "signal").is_some());
}
#[test]
fn withdrawal_closes_admission_and_keeps_same_root_across_busy_and_failure() {
    let mut p = Providers::default();
    let r = request(1);
    register(&mut p, r.clone()).unwrap();
    assert_eq!(
        p.withdraw(caller(), r.id).unwrap().state,
        SourceState::Closing
    );
    assert!(p.find("hub", "signal").is_none());
    let expected = PendId::try_from(r.offered).unwrap();
    p.advance(
        |_, _| false,
        |root| {
            assert_eq!(root, expected);
            Ok(false)
        },
    )
    .unwrap();
    assert_eq!(
        p.advance(
            |_, _| false,
            |root| {
                assert_eq!(root, expected);
                Err(Fail::SourceLost)
            }
        ),
        Err(Fail::SourceLost)
    );
    assert_eq!(
        p.inspect(caller(), r.id).unwrap().state,
        SourceState::Closing
    );
    p.advance(
        |_, _| false,
        |root| {
            assert_eq!(root, expected);
            Ok(true)
        },
    )
    .unwrap();
    assert!(p.closed());
    assert_eq!(
        p.withdraw(caller(), r.id).unwrap().state,
        SourceState::Closed
    );
    assert_eq!(
        p.register(caller(), None, r.clone(), |_| panic!("closed replay"))
            .unwrap()
            .state,
        SourceState::Closed
    );
}
#[test]
fn provider_loss_and_replacement_keep_old_identity_and_reject_token_aliases() {
    let mut p = Providers::default();
    let old = request(1);
    register(&mut p, old.clone()).unwrap();
    p.advance(
        |owner, _| {
            assert_eq!(owner, caller());
            true
        },
        |_| Ok(false),
    )
    .unwrap();
    assert!(p.find("hub", "signal").is_none());
    assert_eq!(register(&mut p, request(2)), Err(Fail::Conflict));
    p.advance(|_, _| false, |_| Ok(true)).unwrap();
    let new = request(2);
    register(&mut p, new.clone()).unwrap();
    assert_eq!(
        p.find("hub", "signal").unwrap().0,
        PendId::try_from(new.offered).unwrap()
    );
    assert_eq!(
        register(&mut p, old.clone()).unwrap().state,
        SourceState::Closed
    );
    let mut alias = request(3);
    alias.offered = old.offered;
    alias.resource.name = "other".into();
    assert_eq!(register(&mut p, alias), Err(Fail::Conflict));
    assert_eq!(
        p.inspect(caller(), new.id).unwrap().state,
        SourceState::Active
    );
}
#[test]
fn full_registry_keeps_replay_and_conflicts_after_retirement() {
    let mut p = Providers::default();
    for id in 1..=64 {
        let mut r = request(id);
        r.resource.name = alloc::format!("signal{id}");
        register(&mut p, r).unwrap();
    }
    p.close();
    p.advance(|_, _| false, |_| Ok(true)).unwrap();
    assert!(p.closed());
    assert_eq!(register(&mut p, request(65)), Err(Fail::Full));
    let mut old = request(1);
    old.resource.name = "signal1".into();
    assert_eq!(register(&mut p, old).unwrap().state, SourceState::Closed);
    assert_eq!(register(&mut p, request(1)), Err(Fail::Conflict));
}

#[test]
fn unseen_withdraw_cancels_late_registration_without_accepting_its_input_grant() {
    let mut p = Providers::default();
    let request = request(1);
    assert_eq!(
        p.withdraw(caller(), request.id).unwrap().state,
        SourceState::Cancelled
    );
    assert_eq!(
        p.register(caller(), Some("hub"), request, |_| panic!(
            "late grant remains with provider"
        ))
        .unwrap()
        .state,
        SourceState::Cancelled
    );
    p.advance(
        |_, _| panic!("no owned root"),
        |_| panic!("no accepted root to retire"),
    )
    .unwrap();
    assert!(p.closed());
    assert!(p.find("hub", "signal").is_none());
}

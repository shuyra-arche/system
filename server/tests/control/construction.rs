use super::{construction_receipts, journal};
use construction_receipts::{Creation, Ledger};
use journal::{Evidence, Intent, Journal};
use kernel_api::model::identity::TaskId as NativeTask;
use system_protocol::{
    Name, TaskId,
    control::instance::*,
    identity::{PrincipalId, Subject},
    loader,
    value::Optional,
};
fn id(value: u64) -> OperationId {
    OperationId::new(value).unwrap()
}
fn instance() -> InstanceId {
    InstanceId {
        system: TaskId::new(3),
        sequence: Sequence::new(1).unwrap(),
    }
}
fn evidence() -> Evidence {
    Evidence {
        constructed: false,
        started: false,
        stopped: false,
        retired: false,
        failure: None,
    }
}
fn create() -> Intent {
    Intent::Create(alloc::boxed::Box::new(Create {
        id: id(1),
        input: loader::Input::Unit(loader::UnitInput {
            name: Name::new("driver").unwrap(),
            stack: 4096,
            arguments: loader::Arguments {
                values: alloc::vec![1],
            },
        }),
        subject: Subject::new(
            PrincipalId {
                authority: TaskId::new(2),
                slot: 0,
            },
            &[],
        )
        .unwrap(),
    }))
}

fn construction_request(id: u64, stack: u64) -> loader::Build {
    loader::Build {
        id: loader::CreateId::new(id).unwrap(),
        input: loader::Input::Unit(loader::UnitInput {
            name: Name::new("worker").unwrap(),
            stack,
            arguments: loader::Arguments {
                values: alloc::vec![1],
            },
        }),
        owner: TaskId::new(4),
        subject: Subject::new(
            PrincipalId {
                authority: TaskId::new(2),
                slot: 0,
            },
            &[],
        )
        .unwrap(),
    }
}

fn sequence(value: u64) -> Sequence {
    Sequence::new(value).unwrap()
}

#[test]
fn construction_ids_replay_exactly_and_are_scoped_to_the_caller() {
    let caller = NativeTask::new(8);
    let other = NativeTask::new(9);
    let request = construction_request(1, 4096);
    let mut ledger = Ledger::new(2).unwrap();

    assert_eq!(ledger.replay(caller, &request), Ok(None));
    ledger.insert(Creation::new(caller, request.clone(), sequence(1), 100));
    assert_eq!(ledger.replay(caller, &request), Ok(Some(0)));
    assert_eq!(
        ledger.replay(caller, &construction_request(1, 8192)),
        Err(loader::Fail::Denied)
    );

    // The same ID belongs to a different caller's independent namespace.
    assert_eq!(ledger.replay(other, &request), Ok(None));
    ledger.insert(Creation::new(other, request.clone(), sequence(2), 100));
    assert_eq!(ledger.replay(other, &request), Ok(Some(1)));
}

#[test]
fn full_construction_ledger_keeps_existing_replay_and_conflict_decisions() {
    let caller = NativeTask::new(8);
    let request = construction_request(1, 4096);
    let mut ledger = Ledger::new(1).unwrap();
    ledger.insert(Creation::new(caller, request.clone(), sequence(1), 100));

    assert_eq!(ledger.replay(caller, &request), Ok(Some(0)));
    assert_eq!(
        ledger.replay(caller, &construction_request(1, 8192)),
        Err(loader::Fail::Denied)
    );
    assert_eq!(
        ledger.replay(caller, &construction_request(2, 4096)),
        Err(loader::Fail::Full)
    );
}

#[test]
fn claimed_construction_keeps_grants_and_ignores_deadline_or_owner_exit() {
    let caller = NativeTask::new(8);
    let mut ledger = Ledger::new(1).unwrap();
    ledger.insert(Creation::new(
        caller,
        construction_request(1, 4096),
        sequence(3),
        10,
    ));

    assert_eq!(ledger.expired_instances(9, |_| false).next(), None);
    assert_eq!(
        ledger.expired_instances(10, |_| false).next(),
        Some(sequence(3))
    );
    assert_eq!(
        ledger.expired_instances(9, |_| true).next(),
        Some(sequence(3))
    );

    let at = ledger
        .find(caller, loader::CreateId::new(1).unwrap())
        .unwrap();
    let resolve_calls = std::cell::Cell::new(0);
    let resolve = ledger
        .resolve_with(at, || {
            resolve_calls.set(resolve_calls.get() + 1);
            Ok::<_, ()>(system_protocol::PendId::new(40))
        })
        .unwrap();
    let replayed_resolve = ledger
        .resolve_with(at, || {
            resolve_calls.set(resolve_calls.get() + 1);
            Ok::<_, ()>(system_protocol::PendId::new(42))
        })
        .unwrap();

    let owner_calls = std::cell::Cell::new(0);
    let owner_resolve = ledger
        .claim_with(at, || {
            owner_calls.set(owner_calls.get() + 1);
            Ok::<_, ()>(system_protocol::PendId::new(41))
        })
        .unwrap();
    let replayed_owner_resolve = ledger
        .claim_with(at, || {
            owner_calls.set(owner_calls.get() + 1);
            Ok::<_, ()>(system_protocol::PendId::new(43))
        })
        .unwrap();

    assert_eq!(resolve, replayed_resolve);
    assert_eq!(owner_resolve, replayed_owner_resolve);
    assert_eq!(resolve_calls.get(), 1);
    assert_eq!(owner_calls.get(), 1);
    assert_eq!(ledger.expired_instances(100, |_| true).next(), None);
}
#[test]
fn accepted_ids_survive_lost_responses_conflict_and_exhaustion() {
    let caller = NativeTask::new(8);
    let mut journal = Journal::new(2).unwrap();
    let (at, fresh) = journal.admit(caller, id(1), create()).unwrap();
    assert!(fresh);
    journal.attach(at, instance());
    assert_eq!(journal.admit(caller, id(1), create()), Ok((at, false)));
    assert_eq!(
        journal.admit(caller, id(1), Intent::Stop(instance())),
        Err(Fail::Conflict)
    );
    assert_eq!(journal.target(at), Some(instance()));
    let mut complete = evidence();
    complete.constructed = true;
    journal.advance(at, complete);
    let receipt = journal.receipt(at);
    assert_eq!(receipt.state, State::Completed);
    assert_eq!(journal.cancel(at), None);
    assert_eq!(journal.receipt(at), receipt);
    let (other, _) = journal
        .admit(NativeTask::new(9), id(1), Intent::Start(instance()))
        .unwrap();
    journal.reject(other, Fail::Denied);
    assert_eq!(journal.find(caller, id(1)), Some(at));
    assert_eq!(journal.find(NativeTask::new(9), id(1)), Some(other));
    assert_eq!(journal.len(), 2);
    assert_eq!(
        journal.admit(caller, id(2), Intent::Stop(instance())),
        Err(Fail::Full)
    );
    assert_eq!(journal.admit(caller, id(1), create()), Ok((at, false)));
    assert_eq!(
        journal.receipt(other).failure.0,
        Some(weave::Coded(Fail::Denied))
    );
}

#[test]
fn selected_owner_is_part_of_requester_intent_and_not_receipt_authority() {
    let Intent::Create(request) = create() else {
        unreachable!()
    };
    let request = CreateFor {
        id: request.id,
        input: request.input,
        owner: TaskId::new(9),
        subject: request.subject,
    };
    let action = Intent::CreateFor(alloc::boxed::Box::new(request.clone()));
    let requester = NativeTask::new(8);
    let owner = NativeTask::new(9);
    let mut journal = Journal::new(1).unwrap();
    let (at, fresh) = journal.admit(requester, id(1), action.clone()).unwrap();
    assert!(fresh);
    journal.attach(at, instance());
    assert_eq!(journal.find(owner, id(1)), None);
    assert_eq!(journal.admit(requester, id(1), action), Ok((at, false)));
    let mut changed = request;
    changed.owner = TaskId::new(10);
    assert_eq!(
        journal.admit(
            requester,
            id(1),
            Intent::CreateFor(alloc::boxed::Box::new(changed))
        ),
        Err(Fail::Conflict)
    );
    assert_eq!(journal.cancel(at), Some(instance()));
    let mut partial = evidence();
    partial.constructed = true;
    partial.stopped = true;
    journal.advance(at, partial);
    assert_eq!(journal.receipt(at).state, State::Pending);
    let mut retired = evidence();
    retired.retired = true;
    journal.advance(at, retired);
    assert_eq!(journal.receipt(at).state, State::Cancelled);
}

#[test]
fn selected_creation_completion_remains_historical() {
    let Intent::Create(request) = create() else {
        unreachable!()
    };
    let request = CreateFor {
        id: request.id,
        input: request.input,
        owner: TaskId::new(9),
        subject: request.subject,
    };
    let mut journal = Journal::new(1).unwrap();
    let (at, _) = journal
        .admit(
            NativeTask::new(8),
            request.id,
            Intent::CreateFor(alloc::boxed::Box::new(request)),
        )
        .unwrap();
    journal.attach(at, instance());
    let mut ready = evidence();
    ready.constructed = true;
    journal.advance(at, ready);
    let completed = journal.receipt(at);
    let mut retired = evidence();
    retired.retired = true;
    journal.advance(at, retired);
    assert_eq!(journal.receipt(at), completed);
    assert_eq!(journal.cancel(at), None);
}
#[test]
fn cancellation_and_retire_require_cleanup_while_stop_does_not() {
    let caller = NativeTask::new(8);
    let mut journal = Journal::new(3).unwrap();
    let (cancel, _) = journal.admit(caller, id(1), create()).unwrap();
    journal.attach(cancel, instance());
    assert_eq!(journal.cancel(cancel), Some(instance()));
    let (stop, _) = journal
        .admit(caller, id(2), Intent::Stop(instance()))
        .unwrap();
    let (retire, _) = journal
        .admit(caller, id(3), Intent::Retire(instance()))
        .unwrap();
    for at in [cancel, stop, retire] {
        let mut stopped = evidence();
        stopped.stopped = true;
        journal.advance(at, stopped);
    }
    assert_eq!(journal.receipt(cancel).state, State::Pending);
    assert_eq!(journal.receipt(stop).state, State::Completed);
    assert_eq!(journal.receipt(retire).state, State::Pending);
    for at in [cancel, retire] {
        let mut retired = evidence();
        retired.retired = true;
        retired.stopped = true;
        journal.advance(at, retired);
    }
    assert_eq!(journal.receipt(cancel).state, State::Cancelled);
    assert_eq!(journal.receipt(retire).state, State::Completed);
}
#[test]
fn publication_receipts_remain_historical() {
    let mut journal = Journal::new(1).unwrap();
    let publication = Publication {
        id: id(1),
        name: Name::new("driver").unwrap(),
        expected: 4,
        instance: Optional(Some(instance())),
    };
    let (at, _) = journal
        .admit(
            NativeTask::new(8),
            id(1),
            Intent::Publish(publication.clone()),
        )
        .unwrap();
    journal.complete(at);
    assert_eq!(journal.receipt(at).state, State::Completed);
    assert_eq!(
        journal.admit(NativeTask::new(8), id(1), Intent::Publish(publication)),
        Ok((at, false))
    );
}

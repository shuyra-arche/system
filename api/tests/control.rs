extern crate alloc;
extern crate system_api as system_protocol;
#[path = "../../server/src/system/control/operations.rs"]
mod journal;
use journal::{Evidence, Intent, Journal};
use kernel_api::model::identity::TaskId as NativeTask;
use system_api::{
    Name, TaskId,
    control::instance::*,
    identity::{PrincipalId, Subject},
    loader,
    value::Optional,
};
use weave::Codec;
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
fn publication_receipts_are_historical_and_nonzero_wire_ids_are_checked() {
    // Discovery is shared by ordinary queries. One large Root list must not
    // force every call to reserve hundreds of KiB in its native reply owner.
    assert!(
        <<system_api::operator::Call as weave::Contract>::Response as Codec>::SIZE
            .maximum()
            .unwrap()
            <= 16 * 1024
    );
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
    assert!(OperationId::decode_exact(&[0; 8]).is_none());
    assert!(Sequence::decode_exact(&[0; 8]).is_none());
    let mut bytes = [0; 16];
    assert_eq!(instance().encode(&mut bytes), Some(16));
    assert_eq!(InstanceId::decode_exact(&bytes), Some(instance()));
    bytes[..8].fill(0);
    assert!(InstanceId::decode_exact(&bytes).is_none());
    for end in 0..16 {
        assert!(InstanceId::decode_exact(&bytes[..end]).is_none());
    }
}

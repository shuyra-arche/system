use system_api::{
    Name, TaskId,
    control::instance::*,
    identity::{PrincipalId, Subject},
    loader,
    value::Optional,
};
use weave::Codec;
fn instance() -> InstanceId {
    InstanceId {
        system: TaskId::new(3),
        sequence: Sequence::new(1).unwrap(),
    }
}
#[test]
fn selected_owner_requests_and_owned_cursors_reject_invalid_frames() {
    let request = CreateFor {
        id: OperationId::new(1).unwrap(),
        input: loader::Input::Unit(loader::UnitInput {
            name: Name::new("driver").unwrap(),
            stack: 4096,
            arguments: loader::Arguments { values: vec![1] },
        }),
        owner: TaskId::new(9),
        subject: Subject::new(
            PrincipalId {
                authority: TaskId::new(2),
                slot: 0,
            },
            &[],
        )
        .unwrap(),
    };
    let mut bytes = vec![0; CreateFor::SIZE.maximum().unwrap()];
    let used = request.encode(&mut bytes).unwrap();
    assert_eq!(CreateFor::decode_exact(&bytes[..used]), Some(request));
    for end in 0..used {
        assert!(CreateFor::decode_exact(&bytes[..end]).is_none());
    }
    let query = Owned {
        after: Optional(Some(instance())),
    };
    let mut bytes = vec![0; Owned::SIZE.maximum().unwrap()];
    let used = query.encode(&mut bytes).unwrap();
    assert_eq!(Owned::decode_exact(&bytes[..used]), Some(query));
    bytes[1..9].fill(0);
    assert!(Owned::decode_exact(&bytes[..used]).is_none());
}
#[test]
fn control_wire_ids_and_discovery_reply_budget_remain_bounded() {
    assert!(
        <<system_api::operator::Call as weave::Contract>::Response as Codec>::SIZE
            .maximum()
            .unwrap()
            <= 16 * 1024
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

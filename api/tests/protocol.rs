use system_api::{
    EntryId, Mark, Name, Path, PendId, TaskId, bootstrap, control, identity, loader, operator,
    retirement, supply,
    value::{Optional, Outcome, Request, Rights},
};
use weave::{Code, Codec};

fn bytes<C: Codec>(value: &C) -> Vec<u8> {
    let mut bytes = vec![0; C::SIZE.maximum().expect("bounded protocol")];
    let used = value.encode(&mut bytes).expect("valid value");
    bytes.truncate(used);
    bytes
}

#[test]
fn operator_grant_keeps_independent_source_constraints_and_nonzero_identity() {
    for (source, source_mark) in [
        (Some(TaskId::new(42)), None),
        (None, Some(Mark::new(u64::MAX))),
        (Some(TaskId::new(42)), Some(Mark::new(u64::MAX))),
    ] {
        let grant = operator::GrantSpec {
            id: operator::GrantId::new(1).unwrap(),
            parent: Optional(None),
            holder: TaskId::new(2),
            anchor: operator::Entry {
                operator: TaskId::new(9),
                target: operator::EntryTarget::Node(EntryId::new(0)),
            },
            scope: operator::Scope::Subtree,
            ops: operator::Ops::BIND,
            permit: Optional(Some(operator::Permit::Bound)),
            source: Optional(source),
            source_mark: Optional(source_mark),
        };
        complete(&grant);
        let encoded = bytes(&grant);
        assert_eq!(operator::GrantSpec::decode_exact(&encoded), Some(grant));
        let mut zero = encoded;
        zero[..8].fill(0);
        assert!(operator::GrantSpec::decode_exact(&zero).is_none());
    }
}

fn complete<C: Codec>(value: &C) {
    let mut encoded = bytes(value);
    assert!(C::decode_exact(&encoded).is_some());
    for end in 0..encoded.len() {
        assert!(C::decode_exact(&encoded[..end]).is_none(), "prefix {end}");
    }
    encoded.push(0);
    assert!(C::decode_exact(&encoded).is_none());
}

fn principal(slot: u64) -> identity::PrincipalId {
    identity::PrincipalId {
        authority: TaskId::new(7),
        slot,
    }
}
fn coalition(slot: u64) -> identity::CoalitionId {
    identity::CoalitionId {
        authority: TaskId::new(7),
        slot,
    }
}
fn subject() -> identity::Subject {
    identity::Subject::new(principal(0), &[coalition(1), coalition(2)]).unwrap()
}

#[test]
fn all_service_families_have_bounded_complete_frames() {
    complete(&bootstrap::Frame {
        version: 2,
        provider: TaskId::new(1),
        system: TaskId::new(1),
    });
    complete(&control::operations::Request::Mint(control::Named {
        name: Name::new("worker").unwrap(),
    }));
    complete(&identity::operations::Request::Adopt(subject()));
    complete(&operator::operations::Request::Seek(operator::Road {
        path: Path::new("identity/resolve").unwrap(),
    }));
    complete(&loader::operations::Request::Inspect(
        loader::CreateId::new(1).unwrap(),
    ));
    complete(&supply::operations::Request::Inspect(supply::Lease {
        lease: supply::LeaseId::new(1).unwrap(),
    }));
    complete(&retirement::operations::Request::Retire(
        retirement::Target { task: TaskId::NONE },
    ));
}

#[test]
fn envelope_rejects_wrong_version_service_and_absent_route() {
    type Envelope = Request<control::operations::Request, 1>;
    let request = Envelope {
        reply: PendId::new(77),
        body: control::operations::Request::State(control::Named {
            name: Name::new("system").unwrap(),
        }),
    };
    complete(&request);
    let encoded = bytes(&request);
    for (index, value) in [(0, 0), (4, 1), (6, 2)] {
        let mut invalid = encoded.clone();
        invalid[index] = value;
        assert!(Envelope::decode_exact(&invalid).is_none());
    }
    let mut invalid = encoded;
    invalid[8..16].fill(0);
    assert!(Envelope::decode_exact(&invalid).is_none());
    let empty = Envelope {
        reply: PendId::NONE,
        body: request.body,
    };
    assert!(empty.encode(&mut [0; 128]).is_none());
}

#[test]
fn unknown_operations_and_failure_codes_do_not_decode_as_success() {
    assert!(control::operations::Request::decode_exact(&[255]).is_none());
    assert!(identity::operations::Request::decode_exact(&[255]).is_none());
    assert!(identity::export::operations::Request::decode_exact(&[255]).is_none());
    assert!(operator::operations::Request::decode_exact(&[255]).is_none());
    assert!(operator::authority::operations::Request::decode_exact(&[255]).is_none());
    assert!(operator::setup::operations::Request::decode_exact(&[255]).is_none());
    assert!(loader::operations::Request::decode_exact(&[255]).is_none());
    assert!(supply::operations::Request::decode_exact(&[255]).is_none());
    assert!(retirement::operations::Request::decode_exact(&[255]).is_none());
    type Reply = Outcome<u64, control::Fail>;
    assert_eq!(Reply::decode_exact(&[1, 255]), None);
    complete(&Reply::Failure(control::Fail::Denied));
    complete(&Reply::Success(0));
}

#[test]
fn bootstrap_requires_the_transport_authenticated_creator() {
    let frame = bootstrap::Frame {
        version: 2,
        provider: TaskId::new(8),
        system: TaskId::new(1),
    };
    assert!(frame.valid_from(TaskId::new(8), TaskId::new(8)));
    assert!(!frame.valid_from(TaskId::new(8), TaskId::new(9)));
    assert!(!frame.valid_from(TaskId::new(9), TaskId::new(9)));
    assert!(!frame.valid_from(TaskId::NONE, TaskId::NONE));
}

#[test]
fn subject_decode_preserves_authority_and_canonical_coalitions() {
    let value = subject();
    complete(&value);
    assert_eq!(
        identity::Subject::decode_exact(&bytes(&value)),
        Some(value.clone())
    );
    let mut invalid = bytes(&value);
    invalid[17..25].copy_from_slice(&8u64.to_le_bytes());
    assert!(identity::Subject::decode_exact(&invalid).is_none());
    let mut invalid = bytes(&value);
    invalid[25..33].copy_from_slice(&2u64.to_le_bytes());
    invalid[41..49].copy_from_slice(&1u64.to_le_bytes());
    assert!(identity::Subject::decode_exact(&invalid).is_none());
    let mut invalid = bytes(&value);
    invalid[16] = 17;
    assert!(identity::Subject::decode_exact(&invalid).is_none());
    assert!(identity::CoalitionSet::new(&[coalition(1), coalition(1)]).is_none());
    assert!(
        identity::Subject::new(
            principal(1),
            &[identity::CoalitionId {
                authority: TaskId::new(8),
                slot: 1
            }]
        )
        .is_none()
    );
}

#[test]
fn identity_requests_reject_cross_authority_relations_and_cursors() {
    let request = identity::Amid {
        principal: principal(1),
        coalition: coalition(2),
    };
    let mut invalid = bytes(&request);
    invalid[16..24].copy_from_slice(&8u64.to_le_bytes());
    assert!(identity::Amid::decode_exact(&invalid).is_none());
    let cursor = identity::Cursor {
        target: identity::PageTarget::Memberships(principal(1)),
        revision: 1,
        after: 1,
    };
    let request = identity::Members {
        coalition: coalition(1),
        cursor: Optional(Some(cursor)),
    };
    assert!(request.encode(&mut [0; 128]).is_none());
}

#[test]
fn pages_reject_wrong_cursor_type_and_threshold() {
    let cursor = identity::Cursor {
        target: identity::PageTarget::Members(coalition(0)),
        revision: 2,
        after: 2,
    };
    let page =
        identity::PrincipalPage::new(&[principal(1), principal(2)], Some(cursor.clone())).unwrap();
    complete(&page);
    let mut invalid = cursor.clone();
    invalid.after = 1;
    assert!(identity::PrincipalPage::new(&[principal(2)], Some(invalid)).is_none());
    assert!(identity::CoalitionPage::new(&[coalition(2)], Some(cursor)).is_none());
    assert!(identity::PrincipalPage::new(&[principal(2), principal(1)], None).is_none());
}

#[test]
fn names_paths_and_scope_bounds_are_explicit() {
    for invalid in ["", ".", "..", "a/b", "a\nb"] {
        assert!(Name::new(invalid).is_none());
    }
    assert!(Name::new(&"x".repeat(31)).is_some());
    assert!(Name::new(&"x".repeat(32)).is_none());
    for invalid in ["/a", "a/", "a//b", "a/../b", "a/b/c/d/e/f/g/h/i"] {
        assert!(Path::new(invalid).is_none());
    }
    complete(&Path::new("").unwrap());
    assert!(operator::Scope::decode_exact(&[1, 0]).is_none());
    assert!(operator::Ops::decode_exact(&u16::MAX.to_le_bytes()).is_none());
    let parent = operator::Scope::Names(operator::Names {
        names: vec![Path::new("a").unwrap()],
    });
    assert!(parent.allows("a/b"));
    assert!(!parent.allows("ab"));
    assert!(!parent.narrows(&operator::Scope::Subtree));
}

#[test]
fn explicit_root_is_distinct_from_entry_zero() {
    let root = bytes(&operator::EntryTarget::Root);
    let first = bytes(&operator::EntryTarget::Node(EntryId::new(0)));
    assert_ne!(root, first);
    complete(&operator::EntryTarget::Root);
    complete(&operator::EntryTarget::Node(EntryId::new(0)));
    complete(&operator::Permit::Opener(EntryId::new(0)));
}

#[test]
fn kernel_rights_use_current_checked_permission_families() {
    use kernel_api::model::{
        modifiable::Modifiable,
        pend::{Distributable, Limits, Operations, PendRange, ShareRights},
    };
    let rights = Rights(Limits {
        operations: Operations::Share(ShareRights::OPEN),
        modifiable: Modifiable::No,
        distributable: Distributable::No,
        range: PendRange::Inherit,
    });
    complete(&rights);
    let mut invalid = bytes(&rights);
    invalid[..8].copy_from_slice(&5u64.to_le_bytes());
    assert!(Rights::decode_exact(&invalid).is_none());
    let mut invalid = bytes(&rights);
    invalid[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(Rights::decode_exact(&invalid).is_none());
}

#[test]
fn local_handles_and_full_width_marks_remain_distinct() {
    let native = kernel_api::model::identity::PendId::from_bytes(&27u64.to_le_bytes()).unwrap();
    let wire = PendId::from(native);
    assert_eq!(
        kernel_api::model::identity::PendId::try_from(wire)
            .unwrap()
            .get(),
        27
    );
    assert!(kernel_api::model::identity::PendId::try_from(PendId::new(u64::MAX)).is_err());
    let mark = Mark::new(u64::MAX);
    complete(&mark);
    let native: kernel_api::model::identity::Mark = mark.into();
    assert_eq!(native.get(), u64::MAX);
}

#[test]
fn operation_ids_never_accept_zero_and_leases_increase() {
    assert!(loader::CreateId::decode_exact(&[0; 8]).is_none());
    assert!(identity::InstallId::decode_exact(&[0; 8]).is_none());
    assert!(identity::FoundId::decode_exact(&[0; 8]).is_none());
    assert!(operator::GrantId::decode_exact(&[0; 8]).is_none());
    assert!(operator::OperationId::decode_exact(&[0; 8]).is_none());
    assert!(supply::LeaseId::decode_exact(&[0; 8]).is_none());
    let mut ids = supply::LeaseIds::default();
    assert_eq!(ids.reserve().unwrap().get(), 1);
    assert_eq!(ids.reserve().unwrap().get(), 2);
}

#[test]
fn bounded_payloads_reject_lengths_before_delivery() {
    let data = supply::Delivery {
        token: PendId::new(1),
        description: vec![0; 128],
    };
    complete(&data);
    let data = supply::Delivery {
        token: PendId::new(1),
        description: vec![0; 129],
    };
    assert!(data.encode(&mut [0; 256]).is_none());
    let args = loader::Arguments {
        values: vec![0; 65],
    };
    assert!(args.encode(&mut [0; 1024]).is_none());
    let mut malformed = vec![0; 8];
    malformed.extend_from_slice(&129u16.to_le_bytes());
    assert!(supply::Delivery::decode_exact(&malformed).is_none());
}

#[test]
fn grant_routes_keep_recovery_and_install_authorities() {
    use control::{Grant, operations::Operation as Control};
    assert_eq!(control::grant(Control::Recover), Grant::Recover);
    assert_eq!(control::grant(Control::ReleaseInstance), Grant::Ruin);
    assert_eq!(control::grant(Control::Observe), Grant::Observe);
    assert_eq!(
        identity::grant(identity::operations::Operation::Associate).code(),
        19
    );
    assert_eq!(
        identity::grant(identity::operations::Operation::Reference).code(),
        20
    );
    let receipt = retirement::Receipt {
        task: TaskId::new(1),
        state: retirement::State::Pending,
    };
    assert!(!receipt.settled());
}

use system_api::{
    EntryId, Name, PendId, TaskId,
    identity::*,
    value::{Optional, Outcome},
};
use weave::{Code, Codec, Coded};

#[test]
fn published_grants_keep_stable_ids_names_and_namespace_admission() {
    let mut names = std::collections::BTreeSet::new();
    for (index, grant) in Grant::ALL.into_iter().enumerate() {
        let mut bytes = [0; 1];
        assert_eq!(Coded(grant).encode(&mut bytes), Some(1));
        assert_eq!(bytes, [index as u8 + 1]);
        assert!(Name::new(grant.name()).is_some());
        assert!(names.insert(grant.name()));
    }
    for grant in [Grant::Bind, Grant::Unbind, Grant::Install] {
        assert_eq!(grant.mount(), Mount::Installer);
    }
    for grant in [
        Grant::Adopt,
        Grant::Waive,
        Grant::Restrict,
        Grant::Derive,
        Grant::Found,
        Grant::Admit,
        Grant::Expel,
        Grant::Activate,
    ] {
        assert_eq!(grant.mount(), Mount::Bound);
    }
    assert_eq!(Grant::Reference.mount(), Mount::Public);
    assert_eq!(Grant::Resolve.mount(), Mount::Public);
}

fn p(slot: u64) -> PrincipalId {
    PrincipalId {
        authority: TaskId::new(5),
        slot,
    }
}
fn c(slot: u64) -> CoalitionId {
    CoalitionId {
        authority: TaskId::new(5),
        slot,
    }
}
fn subject() -> Subject {
    Subject::new(p(0), &[c(0), c(9)]).unwrap()
}
fn entry() -> OperatorEntryRef {
    OperatorEntryRef {
        operator: TaskId::new(44),
        root: false,
        id: EntryId::new(0),
        revision: 6,
    }
}
fn exact<T: Codec>(value: &T) -> Vec<u8> {
    let mut bytes = vec![0; T::SIZE.maximum().unwrap()];
    let length = value.encode(&mut bytes).unwrap();
    bytes.truncate(length);
    assert!(T::decode_exact(&bytes).is_some());
    for end in 0..length {
        assert!(T::decode_exact(&bytes[..end]).is_none());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(T::decode_exact(&trailing).is_none());
    bytes
}

#[test]
fn every_identity_operation_and_install_shape_has_a_complete_codec() {
    use operations::Request as R;
    let task = TaskId::new(8);
    let id = InstallId::new(91).unwrap();
    let actions = vec![
        R::Resolve(Task { task }),
        R::Matches(Matches {
            task,
            selector: Selector::Exact(p(0)),
        }),
        R::Same(Same {
            left: task,
            right: task,
        }),
        R::Sire(p(0)),
        R::Heir(Heir {
            parent: p(0),
            child: p(1),
        }),
        R::Amid(Amid {
            principal: p(0),
            coalition: c(0),
        }),
        R::Members(Members {
            coalition: c(0),
            cursor: Optional(Some(Cursor {
                target: PageTarget::Members(c(0)),
                revision: 3,
                after: 0,
            })),
        }),
        R::Memberships(Memberships {
            principal: p(0),
            cursor: Optional(None),
        }),
        R::Adopt(subject()),
        R::Waive(()),
        R::Restrict(subject()),
        R::Derive(p(0)),
        R::Found(FoundId::new(u64::MAX).unwrap()),
        R::Admit(Amid {
            principal: p(0),
            coalition: c(0),
        }),
        R::Expel(Amid {
            principal: p(0),
            coalition: c(0),
        }),
        R::Bind(Bind {
            task,
            install: Install::Authorized(subject()),
        }),
        R::Unbind(Task { task }),
        R::Activate(Activate {
            task,
            coalitions: CoalitionSet::new(&[c(0), c(9)]).unwrap(),
        }),
        R::Install(InstallRequest {
            id,
            task,
            install: Install::Derive(p(0)),
        }),
        R::Reference(Reference {
            object: IdentityObject::Principal(p(0)),
            name: Name::new("shell").unwrap(),
        }),
        R::Inspect(id),
        R::Cancel(id),
        R::Retire(id),
        R::Associate(Associate {
            object: IdentityObject::Principal(p(0)),
            name: Name::new("shell").unwrap(),
            entry: entry(),
        }),
    ];
    exact(&export::operations::Request::Export(Export {
        object: IdentityObject::Principal(p(0)),
        alias: Name::new("shell").unwrap(),
        binding: Name::new("create").unwrap(),
        entry: entry(),
    }));
    for (index, action) in actions.into_iter().enumerate() {
        let bytes = exact(&action);
        assert_eq!(bytes[0], (index + 1) as u8);
    }
    for install in [
        Install::Inherit(Task { task }),
        Install::Restrict(Restriction {
            parent: task,
            subject: subject(),
        }),
    ] {
        exact(&R::Bind(Bind { task, install }));
    }
    for selector in [Selector::DescendantOf(p(0)), Selector::MemberOf(c(0))] {
        exact(&R::Matches(Matches { task, selector }));
    }
}

#[test]
fn failure_codes_and_grant_routes_remain_explicit() {
    for code in 1..=10 {
        let fail = Fail::from_code(code).unwrap();
        assert_eq!(fail.code(), code);
        assert_eq!(exact(&Outcome::<(), _>::Failure(fail)), [1, code]);
    }
    assert!(Fail::from_code(0).is_none());
    for code in 11..=255 {
        assert!(Fail::from_code(code).is_none());
    }
    for code in 1..=20 {
        let grant = Grant::from_code(code).unwrap();
        assert_eq!(grant.code(), code);
    }
    assert!(Grant::from_code(0).is_none());
    assert!(Grant::from_code(21).is_none());
    for operation in [
        operations::Operation::Inspect,
        operations::Operation::Cancel,
        operations::Operation::Retire,
        operations::Operation::Associate,
    ] {
        assert_eq!(grant(operation), Grant::Install);
    }
}

#[test]
fn v4_envelope_and_selector_have_explicit_wire_goldens() {
    use system_api::value::Request;
    let request = Request::<operations::Request, 2> {
        reply: PendId::new(17),
        body: operations::Request::Activate(Activate {
            task: TaskId::new(8),
            coalitions: CoalitionSet::new(&[c(9)]).unwrap(),
        }),
    };
    let mut golden = b"SYS2".to_vec();
    golden.extend_from_slice(&4u16.to_le_bytes());
    golden.extend_from_slice(&2u16.to_le_bytes());
    golden.extend_from_slice(&17u64.to_le_bytes());
    golden.push(18);
    golden.extend_from_slice(&8u64.to_le_bytes());
    golden.push(1);
    golden.extend_from_slice(&5u64.to_le_bytes());
    golden.extend_from_slice(&9u64.to_le_bytes());
    assert_eq!(exact(&request), golden);
    let mut old = golden.clone();
    for version in [2u16, 3u16] {
        old[4..6].copy_from_slice(&version.to_le_bytes());
        assert!(Request::<operations::Request, 2>::decode_exact(&old).is_none());
    }
    let mut selector = vec![2];
    selector.extend_from_slice(&5u64.to_le_bytes());
    selector.extend_from_slice(&0u64.to_le_bytes());
    assert_eq!(exact(&Selector::MemberOf(c(0))), selector);
    selector[0] = 3;
    assert!(Selector::decode_exact(&selector).is_none());
}

#[test]
fn replies_preserve_zero_slots_installation_state_and_optional_bindings() {
    use operations::Response as R;
    let binding = Binding {
        origin: subject(),
        current: subject(),
    };
    let replies = vec![
        R::Resolve(Outcome::Success(Optional(None))),
        R::Resolve(Outcome::Success(Optional(Some(binding.clone())))),
        R::Sire(Outcome::Success(Optional(Some(p(0))))),
        R::Found(Outcome::Success(c(0))),
        R::Heir(Outcome::Success(true)),
        R::Same(Outcome::Success(MatchReply {
            value: Coded(Match::Unbound),
        })),
        R::Members(Outcome::Success(
            PrincipalPage::new(
                &[p(0)],
                Some(Cursor {
                    target: PageTarget::Members(c(0)),
                    revision: 0,
                    after: 0,
                }),
            )
            .unwrap(),
        )),
        R::Memberships(Outcome::Success(CoalitionPage::new(&[c(0)], None).unwrap())),
        R::Inspect(Outcome::Success(InstallStatus {
            state: Coded(InstallState::Installed),
            binding: Optional(Some(binding)),
            failure: Optional(None),
        })),
        R::Inspect(Outcome::Success(InstallStatus {
            state: Coded(InstallState::Failed),
            binding: Optional(None),
            failure: Optional(Some(Coded(Fail::WrongAuthority))),
        })),
        R::Inspect(Outcome::Success(InstallStatus {
            state: Coded(InstallState::Closed),
            binding: Optional(None),
            failure: Optional(None),
        })),
        R::Reference(Outcome::Success(Optional(Some(entry())))),
        R::Reference(Outcome::Success(Optional(None))),
    ];
    for reply in replies {
        exact(&reply);
    }
    for value in [Match::Yes, Match::No, Match::Unbound] {
        exact(&R::Matches(Outcome::Success(MatchReply {
            value: Coded(value),
        })));
    }
}

#[test]
fn export_rejects_noncanonical_root_and_inherit_rejects_missing_parent() {
    let mut request = Export {
        object: IdentityObject::Principal(p(0)),
        alias: Name::new("shell").unwrap(),
        binding: Name::new("create").unwrap(),
        entry: entry(),
    };
    exact(&request);
    request.entry.root = true;
    request.entry.id = EntryId::new(1);
    assert!(request.encode(&mut [0; 128]).is_none());
    assert!(
        Install::Inherit(Task { task: TaskId::NONE })
            .encode(&mut [0; 128])
            .is_none()
    );
}

#[test]
fn sets_and_pages_never_truncate_and_mutations_preserve_canonical_order() {
    assert!(CoalitionSet::new(&[c(0); 17]).is_none());
    assert!(PrincipalPage::new(&[p(0); 17], None).is_none());
    assert!(
        PrincipalPage::new(
            &[],
            Some(Cursor {
                target: PageTarget::Members(c(0)),
                revision: 0,
                after: 0
            })
        )
        .is_none()
    );
    let values = core::array::from_fn::<_, 16, _>(|index| c(index as u64));
    let mut set = CoalitionSet::new(&values).unwrap();
    assert_eq!(set.len(), 16);
    set.remove(c(0));
    assert_eq!(set.len(), 15);
    assert!(!set.contains(c(0)));
    set.retain(|id| id.slot < 5);
    assert_eq!(set.as_slice(), &[c(1), c(2), c(3), c(4)]);
    assert!(set.is_subset(&CoalitionSet::new(&values).unwrap()));
    exact(&set);
}

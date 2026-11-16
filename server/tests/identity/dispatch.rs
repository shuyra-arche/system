use crate::{
    answer::{self, Context},
    book::IdentityBook,
};
use system_protocol::{
    EntryId, Name, PendId, TaskId,
    identity::{
        operations::{Request, Response},
        *,
    },
    value::{Optional, Outcome},
};
use weave::{Codec, Coded};

fn root() -> PrincipalId {
    PrincipalId {
        authority: TaskId::new(2),
        slot: 0,
    }
}
fn subject() -> Subject {
    Subject::new(root(), &[]).unwrap()
}
fn context(from: u64, grant: Grant) -> Context {
    Context {
        from: TaskId::new(from),
        endpoint: PendId::new(11),
        grant,
    }
}
fn book() -> IdentityBook {
    IdentityBook::new(root().authority, TaskId::new(1)).unwrap()
}

fn call(book: &mut IdentityBook, context: Context, body: Request) -> Response {
    let request = system_protocol::value::Request::<_, 2> {
        reply: PendId::new(19),
        body,
    };
    let mut bytes =
        alloc::vec![0; <system_protocol::value::Request<Request, 2>>::SIZE.maximum().unwrap()];
    let len = weave::codec::encode(&request, &mut bytes).unwrap();
    let request =
        <system_protocol::value::Request<Request, 2>>::decode_exact(&bytes[..len]).unwrap();
    answer::answer(book, context, request.body)
}

fn binding(task: u64) -> Request {
    Request::Bind(Bind {
        task: TaskId::new(task),
        install: Install::Authorized(subject()),
    })
}
fn entry() -> OperatorEntryRef {
    OperatorEntryRef {
        operator: TaskId::new(8),
        root: true,
        id: EntryId::NONE,
        revision: 1,
    }
}

#[test]
fn every_operation_checks_its_endpoint_before_reading_or_mutating_state() {
    let c = CoalitionId {
        authority: root().authority,
        slot: 0,
    };
    let id = InstallId::new(1).unwrap();
    let requests = alloc::vec![
        Request::Resolve(Task {
            task: TaskId::new(3)
        }),
        Request::Matches(Matches {
            task: TaskId::new(3),
            selector: Selector::Exact(root())
        }),
        Request::Same(Same {
            left: TaskId::new(3),
            right: TaskId::new(4)
        }),
        Request::Sire(root()),
        Request::Heir(Heir {
            parent: root(),
            child: root()
        }),
        Request::Amid(Amid {
            principal: root(),
            coalition: c
        }),
        Request::Members(Members {
            coalition: c,
            cursor: Optional(None)
        }),
        Request::Memberships(Memberships {
            principal: root(),
            cursor: Optional(None)
        }),
        Request::Adopt(subject()),
        Request::Waive(()),
        Request::Restrict(subject()),
        Request::Derive(root()),
        Request::Found(FoundId::new(1).unwrap()),
        Request::Admit(Amid {
            principal: root(),
            coalition: c
        }),
        Request::Expel(Amid {
            principal: root(),
            coalition: c
        }),
        binding(3),
        Request::Unbind(Task {
            task: TaskId::new(3)
        }),
        Request::Activate(Activate {
            task: TaskId::new(3),
            coalitions: CoalitionSet::new(&[c]).unwrap()
        }),
        Request::Install(InstallRequest {
            id,
            task: TaskId::new(3),
            install: Install::Authorized(subject())
        }),
        Request::Reference(Reference {
            object: IdentityObject::Principal(root()),
            name: Name::new("shell").unwrap()
        }),
        Request::Inspect(id),
        Request::Cancel(id),
        Request::Retire(id),
        Request::Associate(Associate {
            object: IdentityObject::Principal(root()),
            name: Name::new("shell").unwrap(),
            entry: entry()
        }),
    ];
    let mut book = book();
    for (index, request) in requests.into_iter().enumerate() {
        let wrong = if index == 0 {
            Grant::Found
        } else {
            Grant::Resolve
        };
        let response = call(&mut book, context(1, wrong), request);
        let mut bytes = [0; 16];
        let len = response.encode(&mut bytes).unwrap();
        assert_eq!(&bytes[..len], &[index as u8 + 1, 1, Fail::Denied as u8]);
    }
    assert!(book.resolve(TaskId::new(3)).is_none());
}

#[test]
fn same_keeps_unbound_distinct_from_false_and_uses_native_caller_context() {
    let mut book = book();
    let response = call(
        &mut book,
        context(1, Grant::Same),
        Request::Same(Same {
            left: TaskId::new(3),
            right: TaskId::new(4),
        }),
    );
    assert!(matches!(
        response,
        Response::Same(Outcome::Success(MatchReply {
            value: Coded(Match::Unbound)
        }))
    ));
    // A Bind payload may name a victim, but that does not make its sender the installer.
    assert!(matches!(
        call(&mut book, context(99, Grant::Bind), binding(3)),
        Response::Bind(Outcome::Failure(Fail::Denied))
    ));
    assert!(book.resolve(TaskId::new(3)).is_none());
    assert!(matches!(
        call(&mut book, context(1, Grant::Bind), binding(3)),
        Response::Bind(Outcome::Success(_))
    ));
    assert!(matches!(
        call(&mut book, context(1, Grant::Bind), binding(4)),
        Response::Bind(Outcome::Success(_))
    ));
    let response = call(
        &mut book,
        context(1, Grant::Same),
        Request::Same(Same {
            left: TaskId::new(3),
            right: TaskId::new(4),
        }),
    );
    assert!(matches!(
        response,
        Response::Same(Outcome::Success(MatchReply {
            value: Coded(Match::Yes)
        }))
    ));
    let child = book.derive(TaskId::new(1), root()).unwrap();
    call(
        &mut book,
        context(1, Grant::Bind),
        Request::Bind(Bind {
            task: TaskId::new(4),
            install: Install::Authorized(Subject::new(child, &[]).unwrap()),
        }),
    );
    assert!(matches!(
        call(
            &mut book,
            context(1, Grant::Same),
            Request::Same(Same {
                left: TaskId::new(3),
                right: TaskId::new(4),
            })
        ),
        Response::Same(Outcome::Success(MatchReply {
            value: Coded(Match::No)
        }))
    ));
}

#[test]
fn cancelled_install_cannot_rebind_and_newer_work_does_not_erase_active_installations() {
    let mut book = book();
    let id = InstallId::new(1).unwrap();
    let apply = || {
        Request::Install(InstallRequest {
            id,
            task: TaskId::new(3),
            install: Install::Authorized(subject()),
        })
    };
    for _ in 0..2 {
        assert!(matches!(
            call(&mut book, context(1, Grant::Install), apply()),
            Response::Install(Outcome::Success(InstallStatus {
                state: Coded(InstallState::Installed),
                ..
            }))
        ));
    }
    assert!(matches!(
        call(&mut book, context(1, Grant::Install), Request::Cancel(id)),
        Response::Cancel(Outcome::Success(InstallStatus {
            state: Coded(InstallState::Cancelled),
            ..
        }))
    ));
    assert!(book.resolve(TaskId::new(3)).is_none());
    assert!(matches!(
        call(&mut book, context(1, Grant::Install), apply()),
        Response::Install(Outcome::Success(InstallStatus {
            state: Coded(InstallState::Cancelled),
            ..
        }))
    ));
    assert!(book.resolve(TaskId::new(3)).is_none());
    let second = InstallId::new(2).unwrap();
    call(
        &mut book,
        context(1, Grant::Install),
        Request::Install(InstallRequest {
            id: second,
            task: TaskId::new(4),
            install: Install::Authorized(subject()),
        }),
    );
    assert!(matches!(
        call(&mut book, context(1, Grant::Install), Request::Inspect(id)),
        Response::Inspect(Outcome::Success(InstallStatus {
            state: Coded(InstallState::Closed),
            ..
        }))
    ));
    assert!(matches!(
        call(&mut book, context(1, Grant::Install), apply()),
        Response::Install(Outcome::Failure(Fail::Changed))
    ));
    assert!(book.resolve(TaskId::new(4)).is_some());
    call(
        &mut book,
        context(1, Grant::Install),
        Request::Retire(second),
    );
    assert!(book.resolve(TaskId::new(4)).is_none());
}

#[test]
fn replacement_installation_cannot_be_removed_by_a_cancelled_predecessor() {
    let mut book = book();
    let first = InstallId::new(1).unwrap();
    call(
        &mut book,
        context(1, Grant::Install),
        Request::Install(InstallRequest {
            id: first,
            task: TaskId::new(3),
            install: Install::Authorized(subject()),
        }),
    );
    // Administrative replacement intentionally clears the old installation stamp.
    call(&mut book, context(1, Grant::Bind), binding(3));
    call(
        &mut book,
        context(1, Grant::Install),
        Request::Cancel(first),
    );
    assert!(book.resolve(TaskId::new(3)).is_some());
}

#[test]
fn named_references_require_install_authority_and_preserve_explicit_root() {
    let mut book = book();
    let associate = || {
        Request::Associate(Associate {
            object: IdentityObject::Principal(root()),
            name: Name::new("shell").unwrap(),
            entry: entry(),
        })
    };
    assert!(matches!(
        call(&mut book, context(9, Grant::Install), associate()),
        Response::Associate(Outcome::Failure(Fail::Denied))
    ));
    assert!(matches!(
        call(&mut book, context(1, Grant::Install), associate()),
        Response::Associate(Outcome::Success(()))
    ));
    let result = call(
        &mut book,
        context(7, Grant::Reference),
        Request::Reference(Reference {
            object: IdentityObject::Principal(root()),
            name: Name::new("shell").unwrap(),
        }),
    );
    assert!(
        matches!(result, Response::Reference(Outcome::Success(Optional(Some(actual)))) if actual == entry())
    );
}

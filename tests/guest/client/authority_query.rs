//! Exercise the production read client and namespace admission with native
//! reply grants; no cached identity or synthetic sender substitutes.
use crate::{
    fixture::{self, Console, deadline},
    operator_gate::{Code, Court, Unavailable, verdict},
};
use core::fmt::Write as _;
use kernel_api::model::{
    identity::{PendId, TaskId},
    wait::Wait,
};
use system_client::{
    Error,
    api::{
        self, identity as id,
        operator::{Permit, Wired},
    },
    identity::{QueryError, TaskQuery},
};

pub fn run(
    license: PendId,
    root: TaskId,
    authority: TaskId,
    endpoint: impl Fn(id::Grant) -> PendId,
) {
    let baseline = fixture::active();
    let wired = Wired {
        authority: authority.into(),
        resolve: endpoint(id::Grant::Resolve).into(),
        matches: endpoint(id::Grant::Matches).into(),
        same: endpoint(id::Grant::Same).into(),
    };
    let mut wrong = wired.clone();
    wrong.authority = root.into();
    assert!(matches!(
        TaskQuery::from_wired(wrong, license),
        Err(QueryError::Call(Error::InvalidEndpoint))
    ));
    let query = TaskQuery::from_wired(wired.clone(), license).unwrap();
    let wait = Wait::At(deadline());
    assert!(
        query
            .resolve(id::Task { task: root.into() }, wait)
            .unwrap()
            .is_some()
    );
    let selector = id::Selector::Exact(id::PrincipalId {
        authority: authority.into(),
        slot: 0,
    });
    assert_eq!(
        query
            .matches(
                id::Matches {
                    task: root.into(),
                    selector: selector.clone()
                },
                wait
            )
            .unwrap(),
        id::Match::Yes
    );
    assert_eq!(
        query
            .same(
                id::Same {
                    left: root.into(),
                    right: root.into()
                },
                wait
            )
            .unwrap(),
        id::Match::Yes
    );
    let absent = api::TaskId::new(u64::MAX);
    assert_eq!(
        query
            .same(
                id::Same {
                    left: root.into(),
                    right: absent
                },
                wait
            )
            .unwrap(),
        id::Match::Unbound
    );
    let entry = api::EntryId::new(0);
    let court = Court {
        query: Some(&query),
        openers: |id| Ok((id == entry).then_some(root.into())),
        wait,
    };
    assert_eq!(verdict(&court, root.into(), Permit::Bound), Code::Ok);
    assert_eq!(verdict(&court, absent, Permit::Bound), Code::Denied);
    assert_eq!(
        verdict(&court, root.into(), Permit::Identity(selector)),
        Code::Ok
    );
    assert_eq!(
        verdict(&court, root.into(), Permit::Opener(entry)),
        Code::Ok
    );
    assert_eq!(
        verdict(&court, root.into(), Permit::Opener(api::EntryId::new(1))),
        Code::Unjudged
    );
    let offline = Court {
        query: None,
        openers: |_| Err(Unavailable),
        wait,
    };
    assert_eq!(verdict(&offline, root.into(), Permit::Public), Code::Ok);
    assert_eq!(
        verdict(&offline, root.into(), Permit::Bound),
        Code::Unjudged
    );
    query.finish().unwrap();
    query.finish().unwrap();
    assert_eq!(fixture::active(), baseline);

    // Endpoint ownership alone cannot turn a Matches grant into Resolve.
    // The server rejects it before looking up a binding; the read client must
    // finish every reply grant even for this semantic rejection.
    let mut swapped = wired;
    core::mem::swap(&mut swapped.resolve, &mut swapped.matches);
    let rejected = TaskQuery::from_wired(swapped, license).unwrap();
    assert!(matches!(
        rejected.resolve(id::Task { task: root.into() }, Wait::At(deadline())),
        Err(QueryError::Rejected(id::Fail::Denied))
    ));
    let court = Court {
        query: Some(&rejected),
        openers: |_| Err(Unavailable),
        wait: Wait::At(deadline()),
    };
    assert_eq!(verdict(&court, root.into(), Permit::Bound), Code::Unjudged);
    rejected.finish().unwrap();
    assert_eq!(fixture::active(), baseline);
    writeln!(
        Console,
        "PASS authority-query native=1 sender=1 denied=1 unjudged=1 cleanup=1"
    )
    .unwrap();
}

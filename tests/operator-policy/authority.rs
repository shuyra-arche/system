use super::*;
use system_protocol::{
    EntryId, Mark, PendId as WirePend,
    operator::{Entry, Names, Permit, Scope},
    value::Optional,
};

fn pend(value: u64) -> PendId {
    PendId::from_bytes(&value.to_le_bytes()).unwrap()
}
fn id(value: u64) -> GrantId {
    GrantId::new(value).unwrap()
}
fn path(value: &str) -> Path {
    Path::new(value).unwrap()
}
fn names(values: &[&str]) -> Scope {
    Scope::Names(Names {
        names: values.iter().map(|v| path(v)).collect(),
    })
}
fn setup() -> Authority {
    let mut auth = Authority::new(TaskId::new(1), TaskId::new(9));
    auth.bootstrap_root(RootGrant {
        system: TaskId::new(1),
        operator: TaskId::new(9),
        entrance: pend(90),
    })
    .unwrap();
    auth
}
fn spec(number: u64, holder: u64, parent: Option<u64>) -> GrantSpec {
    GrantSpec {
        id: id(number),
        parent: Optional(parent.map(id)),
        holder: TaskId::new(holder),
        anchor: Entry {
            operator: TaskId::new(9),
            target: EntryTarget::Node(EntryId::new(0)),
        },
        scope: Scope::Subtree,
        ops: Ops::ALL,
        permit: Optional(None),
        source: Optional(None),
        source_mark: Optional(None),
    }
}
fn submit(
    auth: &mut Authority,
    caller: u64,
    via: u64,
    spec: GrantSpec,
    relative: &str,
) -> Result<(GrantState, GrantKey), AuthorityError> {
    auth.grant(
        GrantSubmission {
            caller: TaskId::new(caller),
            via: pend(via),
            spec,
        },
        &path(relative),
    )
}
fn reference(caller: u64, via: u64, number: u64) -> GrantRef {
    GrantRef {
        caller: TaskId::new(caller),
        via: pend(via),
        id: id(number),
    }
}
fn deliver(auth: &mut Authority, key: GrantKey, reference: GrantRef, entrance: u64) {
    auth.set_entrance(reference, pend(entrance)).unwrap();
    auth.attached(key).unwrap();
    auth.delivered(reference, WirePend::new(entrance + 100))
        .unwrap();
}

#[test]
fn root_is_structural_and_canonical_operator_is_not_the_controller() {
    let mut auth = setup();
    let (root, policy) = auth.grant_at(pend(90), TaskId::new(1)).unwrap();
    assert!(policy.is_none());
    assert!(auth.live(root));
    assert!(auth.grant_at(pend(90), TaskId::new(9)).is_none());
    assert_eq!(GrantId::new(0), None);
    let request = spec(1, 2, None);
    assert!(submit(&mut auth, 1, 90, request, "svc").is_ok());
    let mut forged = Authority::new(TaskId::new(1), TaskId::new(9));
    assert_eq!(
        forged.bootstrap_root(RootGrant {
            system: TaskId::new(1),
            operator: TaskId::new(1),
            entrance: pend(90)
        }),
        Err(AuthorityError::Denied)
    );
}

#[test]
fn replay_is_scoped_and_parent_revoke_immediately_blocks_descendants() {
    let mut auth = setup();
    let first = spec(1, 2, None);
    let (_, key) = submit(&mut auth, 1, 90, first.clone(), "svc").unwrap();
    assert_eq!(
        submit(&mut auth, 1, 90, first, "svc"),
        Ok((GrantState::Pending, key))
    );
    let parent_ref = reference(1, 90, 1);
    deliver(&mut auth, key, parent_ref, 91);
    assert_eq!(auth.delivered(parent_ref, WirePend::new(191)), Ok(()));
    assert_eq!(
        auth.delivered(parent_ref, WirePend::new(193)),
        Err(AuthorityError::Conflict)
    );
    let (_, child_key) = submit(&mut auth, 2, 91, spec(1, 3, Some(1)), "driver").unwrap();
    deliver(&mut auth, child_key, reference(2, 91, 1), 92);
    assert!(auth.grant_at(pend(92), TaskId::new(3)).is_some());
    assert!(auth.grant_at(pend(92), TaskId::new(4)).is_none());
    auth.revoke(parent_ref).unwrap();
    assert!(!auth.live(child_key));
    assert!(auth.grant_at(pend(92), TaskId::new(3)).is_none());
    assert!(auth.by_id(child_key, TaskId::new(3)).is_none());
    assert!(auth.remote(child_key).is_none());
    let plan = auth.revoke_plan(parent_ref).unwrap();
    assert_eq!(
        plan.iter().map(|item| item.key).collect::<Vec<_>>(),
        alloc::vec![child_key, key]
    );
    assert_eq!(auth.closed(parent_ref), Err(AuthorityError::Denied));
    for Retirement { key, reference, .. } in plan {
        assert_eq!(
            auth.cleanup_step(key, CleanupStep::Return),
            Err(AuthorityError::Denied)
        );
        auth.cleanup_step(key, CleanupStep::Detach).unwrap();
        assert_eq!(auth.closed(reference), Err(AuthorityError::Denied));
        auth.cleanup_step(key, CleanupStep::Return).unwrap();
        auth.cleanup_step(key, CleanupStep::Return).unwrap();
        auth.closed(reference).unwrap();
    }
    assert_eq!(auth.inspect(parent_ref), Ok(GrantState::Closed));
}

#[test]
fn retirement_order_survives_unrelated_record_compaction() {
    let mut auth = setup();
    // Storage initially holds [unrelated, parent, child]. Closing unrelated
    // swaps child into the first slot; ancestry must still determine order.
    let (_, unrelated) = submit(&mut auth, 1, 90, spec(1, 2, None), "unrelated").unwrap();
    let unrelated_ref = reference(1, 90, 1);
    let (_, parent) = submit(&mut auth, 1, 90, spec(2, 2, None), "svc").unwrap();
    deliver(&mut auth, parent, reference(1, 90, 2), 91);
    let (_, child) = submit(&mut auth, 2, 91, spec(1, 3, Some(2)), "driver").unwrap();
    deliver(&mut auth, child, reference(2, 91, 1), 92);
    auth.revoke(unrelated_ref).unwrap();
    auth.closed(unrelated_ref).unwrap();
    assert!(!auth.live(unrelated));
    let plan = auth.revoke_plan(reference(1, 90, 2)).unwrap();
    assert_eq!(
        plan.iter().map(|item| item.key).collect::<Vec<_>>(),
        alloc::vec![child, parent]
    );
}

#[test]
fn cancel_before_delivery_retains_owner_and_prevents_activation() {
    let mut auth = setup();
    let (_, key) = submit(&mut auth, 1, 90, spec(1, 2, None), "svc").unwrap();
    let reference = reference(1, 90, 1);
    auth.set_entrance(reference, pend(91)).unwrap();
    assert_eq!(
        auth.delivered(reference, WirePend::new(191)),
        Err(AuthorityError::Denied)
    );
    auth.attached(key).unwrap();
    auth.revoke(reference).unwrap();
    assert_eq!(
        auth.delivered(reference, WirePend::new(191)),
        Err(AuthorityError::Denied)
    );
    auth.cleanup_step(key, CleanupStep::Detach).unwrap();
    assert_eq!(
        auth.cleanup_progress(key).unwrap().stage,
        CleanupStage::Detached
    );
    assert_eq!(auth.entrance(reference), Some(pend(91)));
    assert_eq!(auth.closed(reference), Err(AuthorityError::Denied));
    auth.cleanup_step(key, CleanupStep::Return).unwrap();
    auth.closed(reference).unwrap();
}

#[test]
fn unseen_cancel_watermarks_cannot_cross_an_entrance() {
    let mut auth = setup();
    for (number, entrance) in [(1, 91), (2, 92)] {
        let (_, key) = submit(&mut auth, 1, 90, spec(number, 2, None), "svc").unwrap();
        deliver(&mut auth, key, reference(1, 90, number), entrance);
    }
    let cancelled = reference(2, 91, 5);
    assert_eq!(auth.revoke(cancelled), Ok(GrantState::Closed));
    assert_eq!(auth.revoke(cancelled), Ok(GrantState::Closed));
    assert_eq!(
        submit(&mut auth, 2, 91, spec(5, 3, Some(1)), "x"),
        Err(AuthorityError::Stale)
    );
    let request = spec(5, 3, Some(2));
    assert!(submit(&mut auth, 2, 92, request, "x").is_ok());
    assert_eq!(
        submit(&mut auth, 2, 92, spec(5, 4, Some(2)), "x"),
        Err(AuthorityError::Conflict)
    );
}

#[test]
fn each_source_constraint_can_narrow_alone_and_cannot_be_removed() {
    for (task, mark) in [
        (Some(TaskId::new(7)), None),
        (None, Some(Mark::new(u64::MAX))),
        (Some(TaskId::new(7)), Some(Mark::new(u64::MAX))),
    ] {
        let mut auth = setup();
        let mut parent = spec(1, 2, None);
        parent.source = Optional(task);
        parent.source_mark = Optional(mark);
        parent.permit = Optional(Some(Permit::Bound));
        let (_, key) = submit(&mut auth, 1, 90, parent, "svc").unwrap();
        deliver(&mut auth, key, reference(1, 90, 1), 91);
        let mut child = spec(1, 3, Some(1));
        child.source = Optional(task);
        child.source_mark = Optional(mark);
        child.permit = Optional(Some(Permit::Bound));
        child.ops = Ops::BIND;
        let mut no_permit = child.clone();
        no_permit.permit = Optional(None);
        assert_eq!(
            submit(&mut auth, 2, 91, no_permit, "driver"),
            Err(AuthorityError::Denied)
        );
        if task.is_some() {
            let mut wider = child.clone();
            wider.source = Optional(None);
            assert_eq!(
                submit(&mut auth, 2, 91, wider, "driver"),
                Err(AuthorityError::Denied)
            );
        }
        if mark.is_some() {
            let mut wider = child.clone();
            wider.source_mark = Optional(None);
            assert_eq!(
                submit(&mut auth, 2, 91, wider, "driver"),
                Err(AuthorityError::Denied)
            );
        }
        assert!(submit(&mut auth, 2, 91, child, "driver").is_ok());
    }
}

#[test]
fn relocating_anchor_uses_remaining_parent_scope_and_path_boundaries() {
    let parent = names(&["svc/only", "svc/other/deep/x"]);
    assert!(parent.narrows_at(&path("svc/only"), &Scope::Subtree));
    assert!(parent.narrows_at(&path("svc/other/deep"), &names(&["x/y"])));
    assert!(!parent.narrows_at(&path("svc/other/deep"), &Scope::Subtree));
    assert!(!parent.narrows_at(&path("svc"), &names(&["only", "outside"])));
    assert!(!parent.narrows_at(&path("svcx"), &names(&["only"])));
    assert!(!Scope::Subtree.narrows_at(&path("a/b/c/d/e/f/g/h"), &names(&["i"])));
}

#[test]
fn endpoint_aliasing_parent_mismatch_and_key_exhaustion_fail_before_publication() {
    let mut auth = setup();
    let (_, key) = submit(&mut auth, 1, 90, spec(1, 2, None), "svc").unwrap();
    assert_eq!(
        auth.set_entrance(reference(1, 90, 1), pend(90)),
        Err(AuthorityError::Conflict)
    );
    deliver(&mut auth, key, reference(1, 90, 1), 91);
    assert_eq!(
        submit(&mut auth, 2, 91, spec(1, 3, None), "x"),
        Err(AuthorityError::Denied)
    );
    auth.next_key = u64::MAX;
    assert_eq!(
        submit(&mut auth, 2, 91, spec(1, 3, Some(1)), "x"),
        Err(AuthorityError::Full)
    );
    assert_eq!(auth.inspect(reference(2, 91, 1)), Ok(GrantState::Unseen));
    auth.next_key = 2;
    let (_, other) = submit(&mut auth, 2, 91, spec(1, 3, Some(1)), "x").unwrap();
    assert_eq!(
        auth.set_entrance(reference(2, 91, 1), pend(91)),
        Err(AuthorityError::Conflict)
    );
    assert!(!auth.live(other));
}

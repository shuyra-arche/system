use crate::{
    authority::{GrantKey, GrantRef, GrantSubmission, RootGrant},
    tree::{Context, Operator},
};
use kernel_api::model::identity::PendId;
use system_protocol::{
    EntryId, Mark, Name, Path, TaskId,
    operator::{self as api, Entry, EntryTarget, Fail, GrantId, GrantSpec, Ops, Permit, Scope},
    value::Optional,
};
fn pend(raw: u64) -> PendId {
    PendId::from_bytes(&raw.to_le_bytes()).unwrap()
}
fn setup() -> (Operator, Context) {
    let mut tree = Operator::new(TaskId::new(1), TaskId::new(9)).unwrap();
    tree.authority
        .bootstrap_root(RootGrant {
            system: TaskId::new(1),
            operator: TaskId::new(9),
            entrance: pend(90),
        })
        .unwrap();
    (
        tree,
        Context {
            caller: TaskId::new(1),
            via: pend(90),
            authority: GrantKey::Root,
        },
    )
}
fn part(tree: &mut Operator, context: Context, parent: Entry, name: &str) -> api::Binding {
    tree.part(
        context,
        &api::Part {
            change: api::Mutation {
                id: api::OperationId::new(1).unwrap(),
                entry: parent,
                expected: 0,
            },
            name: Name::new(name).unwrap(),
            permit: Permit::Public,
        },
    )
    .unwrap()
}
fn mutation(binding: &api::Binding) -> api::Mutation {
    api::Mutation {
        id: api::OperationId::new(2).unwrap(),
        entry: binding.entry.clone(),
        expected: binding.revision,
    }
}
#[test]
fn root_entry_zero_tombstones_and_stale_revisions_stay_distinct() {
    let (mut tree, context) = setup();
    let root = tree.root();
    let first = part(&mut tree, context, root.clone(), "first");
    assert_eq!(first.entry.target, EntryTarget::Node(EntryId::new(0)));
    assert_ne!(first.entry, root);
    let mut stale = mutation(&first);
    stale.expected += 1;
    assert_eq!(tree.remove(context, &stale, true), Err(Fail::Conflict));
    tree.remove(context, &mutation(&first), true).unwrap();
    let later = part(&mut tree, context, root.clone(), "first");
    assert_ne!(first.entry, later.entry);
    assert_eq!(tree.binding(&first.entry), Err(Fail::Unknown));
    assert_eq!(tree.list(&root).unwrap().ids, alloc::vec![EntryId::new(1)]);
    let replay = part(&mut tree, context, root, "first");
    assert_eq!(replay, later);
    let events = tree.changes();
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<alloc::vec::Vec<_>>(),
        alloc::vec![1, 2, 3]
    );
    assert_eq!(events[1].path.as_str(), "first");
}
#[test]
fn traversal_checks_every_component_and_canonical_operator() {
    let (mut tree, context) = setup();
    let root = tree.root();
    let a = part(&mut tree, context, root.clone(), "a");
    let b = part(&mut tree, context, a.entry.clone(), "b");
    assert_eq!(
        tree.seek(&root, &Path::new("a/b").unwrap()).unwrap(),
        b.entry
    );
    assert_eq!(tree.path(&b.entry).unwrap().as_str(), "a/b");
    assert_eq!(tree.relative(&a.entry, &b.entry).unwrap().as_str(), "b");
    assert_eq!(tree.relative(&b.entry, &a.entry), Err(Fail::Denied));
    let foreign = Entry {
        operator: TaskId::new(8),
        target: a.entry.target.clone(),
    };
    assert_eq!(tree.list(&foreign), Err(Fail::Denied));
    assert_eq!(
        tree.remove(context, &mutation(&a), true),
        Err(Fail::NonEmpty)
    );
}
#[test]
fn pane_and_path_limits_fail_without_a_partial_insert_or_event() {
    let (mut tree, context) = setup();
    let root = tree.root();
    for at in 0..api::PANE_CAP {
        part(&mut tree, context, root.clone(), &alloc::format!("n{at}"));
    }
    let before = tree.changes().len();
    assert_eq!(before, api::PANE_CAP);
    let extra = api::Part {
        change: api::Mutation {
            id: api::OperationId::new(1).unwrap(),
            entry: root.clone(),
            expected: 0,
        },
        name: Name::new("overflow").unwrap(),
        permit: Permit::Public,
    };
    assert_eq!(tree.part(context, &extra), Err(Fail::Full));
    assert!(tree.changes().is_empty());
    assert_eq!(tree.list(&root).unwrap().ids.len(), api::PANE_CAP);
    let mut entry = tree.seek(&root, &Path::new("n0").unwrap()).unwrap();
    for _ in 1..8 {
        entry = part(&mut tree, context, entry, "x").entry;
    }
    let extra = api::Part {
        change: api::Mutation {
            entry,
            ..extra.change
        },
        ..extra
    };
    assert_eq!(tree.part(context, &extra), Err(Fail::Full));
}
#[test]
fn namespace_association_is_controller_only_and_revalidates_its_target() {
    let (mut tree, context) = setup();
    let root = tree.root();
    let pane = part(&mut tree, context, root, "services");
    let association = api::Association {
        mark: Mark::new(kernel_api::model::identity::Mark::of("service").get()),
        name: Name::new("service").unwrap(),
        entry: pane.entry.clone(),
    };
    let outsider = Context {
        caller: TaskId::new(8),
        ..context
    };
    assert_eq!(
        tree.associate(outsider, association.clone()),
        Err(Fail::Denied)
    );
    tree.associate(context, association.clone()).unwrap();
    tree.associate(context, association.clone()).unwrap();
    assert_eq!(
        tree.resolve(association.mark, &association.name).unwrap(),
        pane.entry
    );
    assert_eq!(
        tree.resolve(Mark::new(1), &association.name),
        Err(Fail::Denied)
    );
    tree.remove(context, &mutation(&pane), true).unwrap();
    assert_eq!(
        tree.resolve(association.mark, &association.name),
        Err(Fail::Unknown)
    );
}
#[test]
fn delegated_mutations_cannot_escape_scope_or_outlive_their_parent() {
    let (mut tree, context) = setup();
    let root = tree.root();
    let services = part(&mut tree, context, root.clone(), "services");
    let grant = GrantSpec {
        id: GrantId::new(1).unwrap(),
        parent: Optional(None),
        holder: TaskId::new(2),
        anchor: services.entry.clone(),
        scope: Scope::Names(api::Names {
            names: alloc::vec![Path::new("allowed").unwrap()],
        }),
        ops: Ops::PART,
        permit: Optional(None),
        source: Optional(None),
        source_mark: Optional(None),
    };
    let (_, key) = tree
        .grant(GrantSubmission {
            caller: context.caller,
            via: context.via,
            spec: grant,
        })
        .unwrap();
    let reference = GrantRef {
        caller: context.caller,
        via: context.via,
        id: GrantId::new(1).unwrap(),
    };
    tree.authority.set_entrance(reference, pend(91)).unwrap();
    tree.authority.attached(key).unwrap();
    tree.authority
        .delivered(reference, system_protocol::PendId::new(191))
        .unwrap();
    let delegated = Context {
        caller: TaskId::new(2),
        via: pend(91),
        authority: key,
    };
    let request = |name| api::Part {
        change: api::Mutation {
            id: api::OperationId::new(1).unwrap(),
            entry: services.entry.clone(),
            expected: 0,
        },
        name: Name::new(name).unwrap(),
        permit: Permit::Public,
    };
    assert_eq!(tree.part(delegated, &request("outside")), Err(Fail::Denied));
    let allowed = tree.part(delegated, &request("allowed")).unwrap();
    tree.authority.revoke(reference).unwrap();
    assert_eq!(tree.binding(&allowed.entry), Err(Fail::Unknown));
    assert_eq!(tree.part(delegated, &request("allowed")), Err(Fail::Denied));
    tree.withdraw(key).unwrap();
    assert!(tree.list(&services.entry).unwrap().ids.is_empty());
    assert_eq!(tree.list(&root).unwrap().ids.len(), 1);
}

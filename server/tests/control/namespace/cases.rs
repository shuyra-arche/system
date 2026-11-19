use super::{Installation, Plan};
use crate::{declaration::Unit, effects, real_layout::Layout};
use alloc::vec;
use kernel_api::model::identity::TaskId;
use system_protocol::{operator as op, value::Outcome};
use unit_contract::document as d;

fn unit(publication: bool) -> Unit {
    Unit::compile(d::v2::Unit {
        schema: 2,
        id: "app",
        loader: d::Loader {
            image: "app",
            ..Default::default()
        },
        operator: d::v2::Operator {
            namespace: vec![d::v2::Namespace {
                key: "drivers",
                placement: d::Placement::Instance,
                path: "",
                scope: d::Scope::Subtree,
                ops: op::Ops::ALL.bits(),
                permit: None,
                source_mark: None,
                bindings: if publication {
                    vec![d::v2::Publication {
                        name: "resource",
                        mark: 70,
                        rights: d::Rights {
                            operations: d::rights::Operations::Sign(vec![
                                d::rights::Sign::Trigger,
                            ]),
                            modifiable: false,
                            distributable: true,
                            window: Default::default(),
                        },
                        permit: d::Permit::Public,
                    }]
                } else {
                    Vec::new()
                },
            }],
        },
        ..Default::default()
    })
    .unwrap()
}

fn install(
    installation: &mut Installation,
    layout: &mut Layout,
    services: &effects::Services,
    instance: u64,
    unit: &Unit,
) -> Result<(), op::Fail> {
    installation.install(
        layout,
        services,
        Plan {
            instance,
            task: TaskId::new(instance as usize + 20),
            owner: TaskId::new(instance as usize + 40),
            unit,
        },
        |_, _| Some(kernel_api::model::identity::PendId::mint(77)),
    )
}

fn grant(id: u64, state: op::GrantState) -> Outcome<op::GrantStatus, op::Fail> {
    Outcome::Success(effects::grant_status(op::GrantId::new(id).unwrap(), state))
}

fn grant_id(request: &op::authority::operations::Request) -> Option<op::GrantId> {
    match request {
        op::authority::operations::Request::Revoke(id) => Some(*id),
        _ => None,
    }
}

fn mutation(request: &op::authority::operations::Request) -> Option<op::Mutation> {
    match request {
        op::authority::operations::Request::Unbind(mutation)
        | op::authority::operations::Request::Unpart(mutation) => Some(mutation.clone()),
        _ => None,
    }
}

#[test]
fn partial_install_retains_root_grant_and_containers_until_acknowledged() {
    effects::reset_effects([false, true]);
    let declaration = unit(true);
    let services = effects::Services::new();
    let mut layout = Layout::new();
    let mut installation = Installation::default();
    effects::queue_install(
        &services,
        1,
        TaskId::new(21),
        true,
        Outcome::Failure(op::Fail::Denied),
    );

    assert_eq!(
        install(&mut installation, &mut layout, &services, 1, &declaration),
        Err(op::Fail::Denied)
    );
    assert_eq!(installation.root().unwrap().registration.instance.sequence.get(), 1);
    assert!(installation.anchors.is_empty());

    services.reply(op::authority::operations::Response::Revoke(Outcome::Failure(
        op::Fail::Unjudged,
    )));
    assert_eq!(installation.finish(&mut layout, &services), Err(op::Fail::Unjudged));
    services.reply(op::authority::operations::Response::Revoke(Outcome::Success(
        effects::grant_status(op::GrantId::new(4).unwrap(), op::GrantState::Pending),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());
    services.reply(op::authority::operations::Response::Revoke(Outcome::Success(
        effects::grant_status(op::GrantId::new(4).unwrap(), op::GrantState::Closed),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());

    let revokes: Vec<_> = services.request_log().iter().filter_map(grant_id).collect();
    assert_eq!(revokes, [op::GrantId::new(4).unwrap(); 3]);

    services.reply(op::authority::operations::Response::Unbind(Outcome::Failure(
        op::Fail::Unjudged,
    )));
    assert_eq!(installation.finish(&mut layout, &services), Err(op::Fail::Unjudged));
    services.reply(op::authority::operations::Response::Unbind(Outcome::Success(
        effects::mutation_status(op::OperationState::Pending),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());
    services.reply(op::authority::operations::Response::Unbind(Outcome::Success(
        effects::mutation_status(op::OperationState::Applied),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());

    assert!(!installation.finish(&mut layout, &services).unwrap());
    assert!(!installation.finish(&mut layout, &services).unwrap());
    assert_eq!(
        effects::release_log(),
        [
            kernel_api::model::identity::PendId::mint(501),
            kernel_api::model::identity::PendId::mint(501)
        ]
    );

    services.reply(op::authority::operations::Response::Unpart(Outcome::Success(
        effects::mutation_status(op::OperationState::Pending),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());
    services.reply(op::authority::operations::Response::Unpart(Outcome::Failure(
        op::Fail::Unjudged,
    )));
    assert_eq!(installation.finish(&mut layout, &services), Err(op::Fail::Unjudged));
    services.reply(op::authority::operations::Response::Unpart(Outcome::Success(
        effects::mutation_status(op::OperationState::Applied),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());
    services.reply(op::authority::operations::Response::Unpart(Outcome::Success(
        effects::mutation_status(op::OperationState::Applied),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());
    assert!(installation.finish(&mut layout, &services).unwrap());
    assert!(installation.anchors.is_empty());
    assert_eq!(installation.root().unwrap().registration.instance.sequence.get(), 1);

    let removals: Vec<_> = services
        .request_log()
        .iter()
        .filter_map(|request| match request {
            op::authority::operations::Request::Unpart(mutation) => Some(mutation.id),
            _ => None,
        })
        .collect();
    assert_eq!(removals[0], removals[1]);
    assert_eq!(removals[1], removals[2]);
    assert_ne!(removals[2], removals[3]);
}

#[test]
fn finish_keeps_binding_root_and_container_receipts_until_each_acknowledgement() {
    effects::reset_effects([false, true]);
    let services = effects::Services::new();
    let mut layout = Layout::new();
    let mut installation = Installation::default();
    installation.bindings.push(effects::binding(201));
    installation.roots.push(kernel_api::model::identity::PendId::mint(501));
    installation.containers.push(effects::binding(202));
    installation.containers.push(effects::binding(203));

    services.reply(op::authority::operations::Response::Unbind(Outcome::Success(
        effects::mutation_status(op::OperationState::Pending),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());
    services.reply(op::authority::operations::Response::Unbind(Outcome::Failure(
        op::Fail::Unjudged,
    )));
    assert_eq!(installation.finish(&mut layout, &services), Err(op::Fail::Unjudged));
    services.reply(op::authority::operations::Response::Unbind(Outcome::Success(
        effects::mutation_status(op::OperationState::Applied),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());

    assert!(!installation.finish(&mut layout, &services).unwrap());
    assert!(!installation.finish(&mut layout, &services).unwrap());
    let release_root = kernel_api::model::identity::PendId::mint(501);
    assert_eq!(effects::release_log(), [release_root, release_root]);
    services.reply(op::authority::operations::Response::Unpart(Outcome::Success(
        effects::mutation_status(op::OperationState::Pending),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());
    services.reply(op::authority::operations::Response::Unpart(Outcome::Failure(
        op::Fail::Unjudged,
    )));
    assert_eq!(installation.finish(&mut layout, &services), Err(op::Fail::Unjudged));
    services.reply(op::authority::operations::Response::Unpart(Outcome::Success(
        effects::mutation_status(op::OperationState::Applied),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());
    services.reply(op::authority::operations::Response::Unpart(Outcome::Success(
        effects::mutation_status(op::OperationState::Applied),
    )));
    assert!(!installation.finish(&mut layout, &services).unwrap());
    assert!(installation.finish(&mut layout, &services).unwrap());

    let requests = services.request_log();
    let unbind_ids: Vec<_> = requests
        .iter()
        .filter_map(mutation)
        .take(3)
        .map(|request| request.id)
        .collect();
    assert_eq!(unbind_ids, [unbind_ids[0]; 3]);
    let unpart_ids: Vec<_> = requests
        .iter()
        .filter_map(|request| match request {
            op::authority::operations::Request::Unpart(mutation) => Some(mutation.id),
            _ => None,
        })
        .collect();
    assert_eq!(unpart_ids[0], unpart_ids[1]);
    assert_eq!(unpart_ids[1], unpart_ids[2]);
    assert_ne!(unpart_ids[2], unpart_ids[3]);
}

#[test]
fn instance_cleanup_uses_its_own_root_and_leaves_sibling_installation_intact() {
    effects::reset_effects([]);
    let declaration = unit(false);
    let services = effects::Services::new();
    let mut layout = Layout::new();
    let mut first = Installation::default();
    let mut second = Installation::default();
    effects::queue_install(
        &services,
        3,
        TaskId::new(23),
        false,
        grant(3, op::GrantState::Granted),
    );
    effects::queue_install(
        &services,
        4,
        TaskId::new(24),
        false,
        Outcome::Failure(op::Fail::Denied),
    );

    install(&mut first, &mut layout, &services, 3, &declaration).unwrap();
    assert_eq!(
        install(&mut second, &mut layout, &services, 4, &declaration),
        Err(op::Fail::Denied)
    );
    let first_root = first.root().unwrap();
    let second_root = second.root().unwrap();
    assert_eq!(first_root.registration.instance.sequence.get(), 3);
    assert_eq!(second_root.registration.instance.sequence.get(), 4);
    assert_ne!(
        first_root.registration.pane.entry.target,
        second_root.registration.pane.entry.target
    );

    services.reply(op::authority::operations::Response::Revoke(Outcome::Success(
        effects::grant_status(op::GrantId::new(6).unwrap(), op::GrantState::Closed),
    )));
    assert!(!second.finish(&mut layout, &services).unwrap());
    for _ in 0..2 {
        services.reply(op::authority::operations::Response::Unpart(Outcome::Success(
            effects::mutation_status(op::OperationState::Applied),
        )));
        assert!(!second.finish(&mut layout, &services).unwrap());
    }
    assert!(second.finish(&mut layout, &services).unwrap());

    assert_eq!(first.root(), Some(first_root));
    assert_eq!(first.anchors.len(), 1);
    assert!(second.anchors.is_empty());
    let requests = services.request_log();
    assert_eq!(requests.len(), 11);
    let registrations: Vec<_> = requests
        .iter()
        .filter_map(|request| match request {
            op::authority::operations::Request::RegisterRoot(registration) => {
                Some(registration.instance.sequence.get())
            }
            _ => None,
        })
        .collect();
    assert_eq!(registrations, [3, 4]);
    let second_revokes: Vec<_> = requests.iter().filter_map(grant_id).collect();
    assert_eq!(second_revokes, [op::GrantId::new(6).unwrap()]);
}

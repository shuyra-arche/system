//! The production native supervisor bootstrap, not test-specific service launchers.
use crate::{
    fixture::{self, Console, deadline},
    supervisor_boot::{Phase, Workers},
};
use core::fmt::Write as _;
use kernel_api::model::{
    identity::PendId,
    modifiable::Modifiable,
    pend::{CreationRights, Operations},
    wait::Wait,
};
use kernel_client::call::room;
use system_client::{
    api::{
        self,
        identity::{self as id, operations as identity},
        operator::{self as op, authority::operations as authority, operations as lookup},
        value::Outcome,
    },
    authority::Client as Authority,
    identity::Client as Identity,
    operator::Client as Lookup,
};
macro_rules! rpc {
    ($client:expr, $operation:ty, $body:expr) => {{
        let mut call = $client
            .begin::<$operation>($body, Wait::At(deadline()))
            .unwrap();
        let response = call.wait().unwrap().value;
        while call.finish().is_err() {
            room::yield_();
        }
        response
    }};
}
fn finish(workers: &mut Workers) {
    let end = room::clock().saturating_add(10_000_000_000);
    let mut reported = false;
    loop {
        match workers.finish() {
            Ok(true) => break,
            Ok(false) => (),
            Err(error) => {
                if !reported {
                    writeln!(Console, "bootstrap cleanup pending: {error}").unwrap();
                    reported = true;
                }
            }
        }
        assert!(
            room::clock() < end,
            "bootstrap cleanup timed out: {:?}",
            workers.observations()
        );
        room::yield_();
    }
    assert_eq!(workers.phase(), Phase::Closed);
    assert!(workers.finish().unwrap());
}

pub fn run(license: PendId) {
    let baseline = fixture::active();
    let mut empty = Workers::new(license).unwrap();
    finish(&mut empty);
    assert_eq!(fixture::active(), baseline);
    let mut partial = Workers::new(license).unwrap();
    for _ in 0..2 {
        assert!(!partial.advance().unwrap());
    }
    finish(&mut partial);
    assert_eq!(fixture::active(), baseline);
    let mut starting = Workers::new(license).unwrap();
    fixture::until(|| {
        starting.advance().unwrap();
        starting.phase() == Phase::Operator
    });
    finish(&mut starting);
    assert_eq!(fixture::active(), baseline);
    let restricted = fixture::grant(
        license,
        {
            let kernel_api::model::unit::UnitTarget::Task(task) =
                fixture::inspect(kernel_api::model::unit::UnitQuery::Caller).target
            else {
                unreachable!()
            };
            task
        },
        Operations::None(CreationRights::CLONE),
        Modifiable::No,
    );
    let mut rejected = Workers::new(restricted).unwrap();
    assert!(rejected.advance().is_err());
    assert_eq!(rejected.phase(), Phase::Failed);
    finish(&mut rejected);
    kernel_client::call::pend::return_(restricted).unwrap();
    assert_eq!(fixture::active(), baseline);
    let mut workers = Workers::new(license).unwrap();
    fixture::until(|| workers.advance().unwrap());
    let published = workers.publication().unwrap();
    assert_ne!(published.setup.get(), 0);
    assert!(workers.epoch().current() == 0 && workers.changed().is_some());
    assert_eq!(workers.revision().unwrap(), 0);
    let mut identity = Identity::bind(
        workers.identity_face(id::Grant::Bind).unwrap(),
        published.identity,
        license,
    )
    .unwrap();
    let controller = fixture::inspect(kernel_api::model::unit::UnitQuery::Caller).target;
    let kernel_api::model::unit::UnitTarget::Task(controller) = controller else {
        unreachable!()
    };
    let subject = id::Subject::new(
        id::PrincipalId {
            authority: published.identity.into(),
            slot: 0,
        },
        &[],
    )
    .unwrap();
    assert!(matches!(
        rpc!(
            identity,
            identity::Bind,
            id::Bind {
                task: controller.into(),
                install: id::Install::Authorized(subject)
            }
        ),
        Outcome::Success(_)
    ));
    let mut resolver = Identity::bind(
        workers.identity_face(id::Grant::Resolve).unwrap(),
        published.identity,
        license,
    )
    .unwrap();
    assert!(matches!(
        rpc!(
            resolver,
            identity::Resolve,
            id::Task {
                task: controller.into()
            }
        ),
        Outcome::Success(_)
    ));
    fixture::until(|| workers.revision().unwrap() == 1);
    let root = op::Entry {
        operator: published.operator.into(),
        target: op::EntryTarget::Root,
    };
    let mut authority = Authority::bind(published.authority, published.operator, license).unwrap();
    let Outcome::Success(child) = rpc!(
        authority,
        authority::Part,
        op::Part {
            change: op::Mutation {
                id: op::OperationId::new(1).unwrap(),
                entry: root.clone(),
                expected: 0
            },
            name: api::Name::new("bootstrap").unwrap(),
            permit: op::Permit::Public,
        }
    ) else {
        panic!("bootstrap root authority")
    };
    let mut lookup = Lookup::bind(published.discovery, published.operator, license).unwrap();
    let result = rpc!(
        lookup,
        lookup::Seek,
        op::Road {
            path: api::Path::new("bootstrap").unwrap()
        }
    );
    assert!(matches!(result, Outcome::Success(binding) if binding == child.entry));
    finish(&mut workers);
    assert!(workers.publication().is_none());
    for receipt in workers.observations().into_iter().flatten() {
        assert_eq!(
            receipt.exit,
            Some((kernel_api::model::unit::ExitCause::Reap, 0))
        );
        assert_eq!(receipt.result, Some(0));
        assert!(receipt.reclaimed);
    }
    assert_eq!(
        fixture::active(),
        baseline,
        "native bootstrap left live caller-local resources"
    );
    let mut failed = Workers::new(license).unwrap();
    fixture::until(|| failed.advance().unwrap());
    let publication = failed.publication().unwrap();
    kernel_client::call::unit::stop(publication.operator.into()).unwrap();
    fixture::until(|| failed.advance().is_err());
    assert_eq!(failed.phase(), Phase::Failed);
    finish(&mut failed);
    assert_eq!(
        fixture::active(),
        baseline,
        "worker death lost bootstrap ownership"
    );
    writeln!(
        Console,
        "PASS supervisor-bootstrap native=1 roles=2 root=1 wired=1 revision=1 partial=1 cleanup=1"
    )
    .unwrap();
}

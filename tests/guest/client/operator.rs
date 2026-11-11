//! Whole production Operator service, real sender Tasks and native grants.
use crate::{
    fixture::{self, Console, addr, deadline, until},
    operator_runtime::{Config, Service},
};
use core::{
    fmt::Write as _,
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use kernel_api::model::{
    clone_view::CloneLimits,
    identity::{Mark, PendId, TaskId},
    modifiable::Modifiable,
    pend::{
        AccordRequest, AccordTarget, CloneRights, CreationRights, Distributable, Limits,
        Operations, PendRange,
    },
    sign::SignRights,
    unit::UnitQuery,
    wait::Wait,
};
use kernel_client::call::{clone_view, pend, room, sign, unit};
use system_client::{
    api::{
        self, identity as id,
        operator::{
            self as op, authority::operations as auth, operations as lookup,
            setup::operations as setup,
        },
        value::{Optional, Outcome, Rights},
    },
    authority::Client as Authority,
    operator::Client as Lookup,
    setup::Client as Setup,
};
use twine::view::clone::{Receiver, Reply, Sender};
use weave::transport;
static READY: AtomicBool = AtomicBool::new(false);
static DONE: AtomicBool = AtomicBool::new(false);
static STEP: AtomicUsize = AtomicUsize::new(0);
static SHUTDOWN: AtomicUsize = AtomicUsize::new(0);
static HOST: AtomicUsize = AtomicUsize::new(0);
static PUBLICATION: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
static USER: [AtomicUsize; 5] = [const { AtomicUsize::new(0) }; 5];
static BINDING: [AtomicU64; 2] = [const { AtomicU64::new(0) }; 2];
static REPLY_FAILURES: AtomicUsize = AtomicUsize::new(0);
const RESOURCE_MARK: u64 = u64::MAX - 7;
macro_rules! rpc {
    ($client:expr, $operation:ty, $body:expr) => {{
        let mut pending = $client
            .begin::<$operation>($body, Wait::At(deadline()))
            .unwrap();
        let value = pending.wait().unwrap().value;
        pending.finish().unwrap();
        value
    }};
}
fn success<T>(result: Outcome<T, op::Fail>) -> T {
    match result {
        Outcome::Success(value) => value,
        Outcome::Failure(error) => panic!("operator response {error:?}"),
    }
}
fn native(value: usize) -> PendId {
    PendId::from_bytes(&(value as u64).to_le_bytes()).unwrap()
}
fn rights(distributable: Distributable) -> Limits {
    Limits {
        operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
        modifiable: Modifiable::Yes,
        distributable,
        range: PendRange::Inherit,
    }
}
fn grant(source: PendId, target: TaskId, limits: Limits) -> PendId {
    let request = AccordRequest {
        src: source,
        target: AccordTarget::Task(target),
        mark: Mark::NONE,
        limits,
    };
    let mut result = None;
    until(|| match pend::accord(addr(&request.to_bytes())) {
        Ok(token) => {
            result = Some(token);
            true
        }
        Err(error) if error.source.is_busy() => false,
        Err(error) => panic!("operator grant {error:?}"),
    });
    result.unwrap()
}
fn root(host: TaskId) -> op::Entry {
    op::Entry {
        operator: host.into(),
        target: op::EntryTarget::Root,
    }
}
fn mutation(entry: op::Entry, id: u64, expected: u64) -> op::Mutation {
    op::Mutation {
        id: op::OperationId::new(id).unwrap(),
        entry,
        expected,
    }
}
pub fn run(
    license: PendId,
    controller: TaskId,
    identity: TaskId,
    endpoint: impl Fn(id::Grant) -> PendId,
) {
    READY.store(false, Ordering::Release);
    DONE.store(false, Ordering::Release);
    STEP.store(0, Ordering::Release);
    REPLY_FAILURES.store(0, Ordering::Release);
    let baseline = fixture::active();
    let stop = sign::create(license).unwrap();
    let (target, host) = fixture::spawn(worker);
    HOST.store(host.get(), Ordering::Release);
    fixture::grant(
        license,
        host,
        Operations::None(CreationRights::CLONE | CreationRights::SIGN),
        Modifiable::No,
    );
    SHUTDOWN.store(
        fixture::grant(
            stop,
            host,
            Operations::Sign(SignRights::LISTEN | SignRights::CONSUME | SignRights::ROUTE),
            Modifiable::No,
        )
        .get(),
        Ordering::Release,
    );
    unit::resume(target).unwrap();
    until(|| READY.load(Ordering::Acquire));
    let publication = |index: usize| native(PUBLICATION[index].load(Ordering::Acquire));
    let mut setup = Setup::bind(publication(2), host, license).unwrap();
    let mut authority = Authority::bind(publication(1), host, license).unwrap();
    let mut lookup = Lookup::bind(publication(0), host, license).unwrap();
    success(rpc!(setup, setup::Root, ()));
    let wired = op::Wired {
        authority: identity.into(),
        resolve: grant(
            endpoint(id::Grant::Resolve),
            host,
            rights(Distributable::No),
        )
        .into(),
        matches: grant(
            endpoint(id::Grant::Matches),
            host,
            rights(Distributable::No),
        )
        .into(),
        same: grant(endpoint(id::Grant::Same), host, rights(Distributable::No)).into(),
    };
    success(rpc!(setup, setup::Wired, wired));
    let services = success(rpc!(
        authority,
        auth::Part,
        op::Part {
            change: mutation(root(host), 1, 0),
            name: api::Name::new("services").unwrap(),
            permit: op::Permit::Public
        }
    ));
    success(rpc!(
        authority,
        auth::Associate,
        op::Association {
            mark: api::Mark::new(Mark::of("service").get()),
            name: api::Name::new("service").unwrap(),
            entry: services.entry.clone()
        }
    ));
    assert_eq!(
        success(rpc!(
            lookup,
            lookup::Resolve,
            op::Resolve {
                mark: api::Mark::new(Mark::of("service").get()),
                name: api::Name::new("service").unwrap()
            }
        )),
        services.entry
    );
    let watch = clone_view::create(
        license,
        Mark::NONE,
        CloneLimits {
            max_len: 1024,
            max_messages: 8,
            max_bytes: 8192,
        },
    )
    .unwrap();
    let watch_route = grant(watch, host, rights(Distributable::No));
    success(rpc!(
        lookup,
        lookup::Watch,
        op::Watch {
            base: root(host),
            path: api::Path::new("services").unwrap(),
            endpoint: watch_route.into()
        }
    ));
    let source = clone_view::create(
        license,
        Mark::new(RESOURCE_MARK),
        CloneLimits {
            max_len: 1024,
            max_messages: 8,
            max_bytes: 8192,
        },
    )
    .unwrap();
    let (user_target, user) = fixture::spawn(user_worker);
    fixture::grant(
        license,
        user,
        Operations::None(CreationRights::CLONE | CreationRights::SIGN),
        Modifiable::No,
    );
    USER[0].store(
        grant(source, user, rights(Distributable::Yes)).get(),
        Ordering::Release,
    );
    let op::EntryTarget::Node(anchor) = services.entry.target else {
        unreachable!()
    };
    USER[1].store(anchor.get() as usize, Ordering::Release);
    USER[2].store(controller.get(), Ordering::Release);
    let grant_id = op::GrantId::new(1).unwrap();
    success(rpc!(
        authority,
        auth::Grant,
        op::GrantSpec {
            id: grant_id,
            parent: Optional(None),
            holder: user.into(),
            anchor: services.entry.clone(),
            scope: op::Scope::Names(op::Names {
                names: alloc::vec![api::Path::new("allowed").unwrap()]
            }),
            ops: op::Ops::PART
                .union(op::Ops::BIND)
                .union(op::Ops::UNBIND)
                .union(op::Ops::INSPECT),
            permit: Optional(Some(op::Permit::Public)),
            source: Optional(Some(controller.into())),
            source_mark: Optional(Some(api::Mark::new(RESOURCE_MARK)))
        }
    ));
    let mut remote = None;
    until(|| {
        let status = success(rpc!(authority, auth::Inspect, grant_id));
        if status.state.0 == op::GrantState::Granted {
            remote = status.entrance.0;
            true
        } else {
            assert_eq!(status.state.0, op::GrantState::Pending);
            false
        }
    });
    USER[3].store(remote.unwrap().get() as usize, Ordering::Release);
    unit::resume(user_target).unwrap();
    until(|| STEP.load(Ordering::Acquire) == 1);
    let binding = op::Binding {
        entry: op::Entry {
            operator: host.into(),
            target: op::EntryTarget::Node(api::EntryId::new(BINDING[0].load(Ordering::Acquire))),
        },
        revision: BINDING[1].load(Ordering::Acquire),
    };
    let found = success(rpc!(
        lookup,
        lookup::Find,
        op::At {
            entry: binding.entry.clone()
        }
    ));
    let consumer: PendId = found.token.try_into().unwrap();
    Sender::new(consumer).push(b"before").unwrap();
    let mut receiver = Receiver::new(source);
    let mut bytes = [0; 1024];
    let mut before = None;
    until(
        || match weave::FrameReceiver::receive(&mut receiver, &mut bytes) {
            Ok(message) => {
                assert_eq!(&bytes[..message.len], b"before");
                before = Some(message.metadata);
                true
            }
            Err(_) => false,
        },
    );
    assert_eq!(before.unwrap(), controller);
    STEP.store(2, Ordering::Release);
    until(|| STEP.load(Ordering::Acquire) == 3);
    assert!(matches!(
        rpc!(
            lookup,
            lookup::Find,
            op::At {
                entry: binding.entry.clone()
            }
        ),
        Outcome::Failure(op::Fail::Unknown)
    ));
    Sender::new(consumer).push(b"after").unwrap();
    until(
        || match weave::FrameReceiver::receive(&mut receiver, &mut bytes) {
            Ok(message) => {
                assert_eq!(&bytes[..message.len], b"after");
                true
            }
            Err(_) => false,
        },
    );
    let once = op::Part {
        change: mutation(services.entry.clone(), 100, 0),
        name: api::Name::new("once").unwrap(),
        permit: op::Permit::Public,
    };
    let mut reply = Reply::create(license, 1024).unwrap();
    let route = reply.grant(host).unwrap();
    let kernel_api::model::sign::SignSource::Clone { token, .. } = reply.source() else {
        unreachable!()
    };
    Sender::new(token).push(&[255]).unwrap();
    let request = api::value::Request::<_, 6> {
        reply: route.into(),
        body: auth::Request::Part(once.clone()),
    };
    transport::send(&mut Sender::new(publication(1)), &request, &mut [0; 4096]).unwrap();
    until(|| REPLY_FAILURES.load(Ordering::Acquire) == 1);
    reply.finish().unwrap();
    let original_once = success(rpc!(
        authority,
        auth::Find,
        op::Query {
            anchor: services.entry.clone(),
            name: api::Name::new("once").unwrap()
        }
    ));
    success(rpc!(
        authority,
        auth::Unpart,
        mutation(original_once.entry.clone(), 101, original_once.revision)
    ));
    let once_binding = success(rpc!(authority, auth::Part, once));
    assert_eq!(once_binding, original_once);
    let list = success(rpc!(
        lookup,
        lookup::List,
        op::At {
            entry: services.entry.clone()
        }
    ));
    let allowed = success(rpc!(
        lookup,
        lookup::Relative,
        op::Relative {
            base: services.entry.clone(),
            path: api::Path::new("allowed").unwrap()
        }
    ));
    let op::EntryTarget::Node(allowed_id) = allowed.target else {
        unreachable!()
    };
    assert_eq!(
        list.ids,
        alloc::vec![allowed_id],
        "retry re-executed a removed mutation"
    );
    let mut events = Receiver::new(watch);
    let mut seen = [0; 4];
    let end = deadline();
    while seen.contains(&0) {
        assert!(room::clock() < end, "missing Operator event {seen:?}");
        if let Ok(event) = transport::receive::<op::Event, _>(&mut events, &mut bytes) {
            assert_eq!(event.metadata, host);
            match (event.value.kind.0, event.value.path.as_str()) {
                (op::Kind::Landed, "services/allowed/resource") => seen[0] += 1,
                (op::Kind::Trimmed, "services/allowed/resource") => seen[1] += 1,
                (op::Kind::Parted, "services/once") => seen[2] += 1,
                (op::Kind::Trimmed, "services/once") => seen[3] += 1,
                _ => (),
            }
        } else {
            room::yield_();
        }
    }
    assert_eq!(seen, [1, 1, 1, 1]);
    events.discard().unwrap();
    receiver.discard().unwrap();
    let nested_id = op::GrantId::new(2).unwrap();
    success(rpc!(
        authority,
        auth::Grant,
        op::GrantSpec {
            id: nested_id,
            parent: Optional(None),
            holder: user.into(),
            anchor: allowed,
            scope: op::Scope::Names(op::Names {
                names: alloc::vec![api::Path::new("nested").unwrap()]
            }),
            ops: op::Ops::PART,
            permit: Optional(None),
            source: Optional(None),
            source_mark: Optional(None)
        }
    ));
    let mut nested_route = None;
    until(|| {
        let status = success(rpc!(authority, auth::Inspect, nested_id));
        if status.state.0 == op::GrantState::Granted {
            nested_route = status.entrance.0;
            true
        } else {
            assert_eq!(status.state.0, op::GrantState::Pending);
            false
        }
    });
    USER[4].store(nested_route.unwrap().get() as usize, Ordering::Release);
    STEP.store(4, Ordering::Release);
    until(|| STEP.load(Ordering::Acquire) == 5);
    success(rpc!(authority, auth::Revoke, grant_id));
    assert_eq!(
        success(rpc!(authority, auth::Inspect, grant_id)).state.0,
        op::GrantState::Revoking
    );
    success(rpc!(authority, auth::Revoke, nested_id));
    until(|| success(rpc!(authority, auth::Inspect, nested_id)).state.0 == op::GrantState::Closed);
    until(|| success(rpc!(authority, auth::Inspect, grant_id)).state.0 == op::GrantState::Closed);
    assert!(Sender::new(consumer).push(b"revoked").is_err());
    STEP.store(6, Ordering::Release);
    until(|| fixture::inspect(UnitQuery::Target(user_target)).reclaimed);
    assert_eq!(
        fixture::inspect(UnitQuery::Target(user_target)).result,
        Some(7)
    );
    unit::delete(user_target).unwrap();
    let doomed = success(rpc!(
        authority,
        auth::Bind,
        op::Bind {
            change: mutation(services.entry.clone(), 102, 0),
            name: api::Name::new("dead").unwrap(),
            resource: grant(source, host, rights(Distributable::Yes)).into(),
            resource_id: 102,
            rights: Rights(rights(Distributable::No)),
            permit: op::Permit::Public,
            source: op::Source {
                task: controller.into(),
                mark: api::Mark::new(RESOURCE_MARK)
            },
        }
    ));
    until(|| match pend::return_(source) {
        Ok(()) => true,
        Err(error) if error.source.is_busy() => false,
        Err(error) => panic!("source withdrawal: {error:?}"),
    });
    assert!(matches!(
        rpc!(
            lookup,
            lookup::Find,
            op::At {
                entry: doomed.entry.clone()
            }
        ),
        Outcome::Failure(op::Fail::Dead)
    ));
    assert!(matches!(
        rpc!(
            lookup,
            lookup::Find,
            op::At {
                entry: doomed.entry
            }
        ),
        Outcome::Failure(op::Fail::Unknown)
    ));
    sign::trigger(stop).unwrap();
    until(|| DONE.load(Ordering::Acquire));
    until(|| fixture::inspect(UnitQuery::Target(target)).reclaimed);
    assert_eq!(fixture::inspect(UnitQuery::Target(target)).result, Some(42));
    unit::delete(target).unwrap();
    pend::return_(stop).unwrap();
    pend::return_(watch).unwrap();
    assert_eq!(
        fixture::active(),
        baseline,
        "Operator left caller-owned grants live"
    );
    writeln!(Console, "PASS operator-service native=1 namespace=1 scope=1 source=1 unbind=1 revoke=1 replay=1 watch=1 cleanup=1").unwrap();
}
extern "C" fn worker() -> ! {
    let baseline = fixture::active();
    let mut service = Service::create(Config {
        license: fixture::license(),
        controller: TaskId::new(crate::ROOT.load(Ordering::Acquire)),
        shutdown: native(SHUTDOWN.load(Ordering::Acquire)),
    })
    .unwrap();
    let publication = service.publication().unwrap();
    for (cell, token) in PUBLICATION.iter().zip([
        publication.discovery,
        publication.authority,
        publication.setup,
    ]) {
        cell.store(token.get() as usize, Ordering::Release);
    }
    READY.store(true, Ordering::Release);
    loop {
        let report = service.step(Wait::Infinite).unwrap();
        REPLY_FAILURES.fetch_add(report.reply_failures, Ordering::AcqRel);
        if report.stopped {
            break;
        }
    }
    until(|| service.finish().is_ok());
    service.finish().unwrap();
    assert_eq!(
        fixture::active(),
        baseline + 3,
        "Operator leaked owned resources or imported publication roots"
    );
    DONE.store(true, Ordering::Release);
    fixture::exit(42)
}
extern "C" fn user_worker() -> ! {
    let host = TaskId::new(HOST.load(Ordering::Acquire));
    let license = fixture::license();
    let mut authority =
        Authority::bind(native(USER[3].load(Ordering::Acquire)), host, license).unwrap();
    let resource = grant(
        native(USER[0].load(Ordering::Acquire)),
        host,
        rights(Distributable::Yes),
    );
    let anchor = op::Entry {
        operator: host.into(),
        target: op::EntryTarget::Node(api::EntryId::new(USER[1].load(Ordering::Acquire) as u64)),
    };
    let allowed = success(rpc!(
        authority,
        auth::Part,
        op::Part {
            change: mutation(anchor.clone(), 1, 0),
            name: api::Name::new("allowed").unwrap(),
            permit: op::Permit::Public
        }
    ));
    let forged = op::Bind {
        change: mutation(allowed.entry.clone(), 2, 0),
        name: api::Name::new("resource").unwrap(),
        resource: resource.into(),
        resource_id: 1,
        rights: Rights(rights(Distributable::No)),
        permit: op::Permit::Public,
        source: op::Source {
            task: api::TaskId::new(USER[2].load(Ordering::Acquire) as u64),
            mark: api::Mark::NONE,
        },
    };
    assert!(matches!(
        rpc!(authority, auth::Bind, forged),
        Outcome::Failure(op::Fail::Denied)
    ));
    assert!(matches!(
        rpc!(authority, auth::Cancel, op::OperationId::new(1).unwrap()),
        Outcome::Failure(op::Fail::Denied)
    ));
    let resource = grant(
        native(USER[0].load(Ordering::Acquire)),
        host,
        rights(Distributable::Yes),
    );
    let binding = success(rpc!(
        authority,
        auth::Bind,
        op::Bind {
            change: mutation(allowed.entry.clone(), 3, 0),
            name: api::Name::new("resource").unwrap(),
            resource: resource.into(),
            resource_id: 1,
            rights: Rights(rights(Distributable::No)),
            permit: op::Permit::Public,
            source: op::Source {
                task: api::TaskId::new(USER[2].load(Ordering::Acquire) as u64),
                mark: api::Mark::new(RESOURCE_MARK)
            }
        }
    ));
    let op::EntryTarget::Node(id) = binding.entry.target else {
        unreachable!()
    };
    BINDING[0].store(id.get(), Ordering::Release);
    BINDING[1].store(binding.revision, Ordering::Release);
    assert!(matches!(
        rpc!(
            authority,
            auth::Part,
            op::Part {
                change: mutation(anchor, 4, 0),
                name: api::Name::new("outside").unwrap(),
                permit: op::Permit::Public
            }
        ),
        Outcome::Failure(op::Fail::Denied)
    ));
    STEP.store(1, Ordering::Release);
    until(|| STEP.load(Ordering::Acquire) == 2);
    let status = success(rpc!(
        authority,
        auth::Unbind,
        mutation(binding.entry, 5, binding.revision)
    ));
    assert_eq!(status.state.0, op::OperationState::Applied);
    STEP.store(3, Ordering::Release);
    until(|| STEP.load(Ordering::Acquire) == 4);
    let mut child =
        Authority::bind(native(USER[4].load(Ordering::Acquire)), host, license).unwrap();
    success(rpc!(
        child,
        auth::Part,
        op::Part {
            change: mutation(allowed.entry, 1, 0),
            name: api::Name::new("nested").unwrap(),
            permit: op::Permit::Public
        }
    ));
    STEP.store(5, Ordering::Release);
    until(|| STEP.load(Ordering::Acquire) == 6);
    fixture::exit(7)
}

//! Independent ordinary User Teams consuming the production System handoff.
#![no_std]
#![no_main]
extern crate alloc;
// This fixture uses the production allocator, but creates no same-Team workers.
#[allow(dead_code)]
#[path = "../../../server/src/runtime/heap/mod.rs"]
pub mod heap;
#[path = "../../../server/src/runtime/memory.rs"]
pub mod memory;
#[path = "../../../server/src/runtime/termination.rs"]
pub mod termination;
pub mod runtime {
    pub use crate::{memory, termination};
}
mod install;
mod owner;
#[path = "../../../server/src/protocol/mod.rs"]
pub mod protocol;
mod provider;
#[path = "../../../server/src/system/supply/native.rs"]
pub mod source;
use alloc::{string::String, vec};
use core::{
    fmt::{self, Write},
    sync::atomic::{AtomicU64, Ordering},
};
use kernel_api::{
    address::VirtAddr,
    model::{
        clone_view::CloneLimits,
        identity::{Mark, PendId, TaskId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, CloneRights, Distributable, Limits, Operations, PendInfo,
            PendScope, Target,
        },
        sign::SignRights,
        unit::{ExitScope, UnitInfo, UnitQuery, UnitTarget},
        wait::Wait,
    },
};
use kernel_client::call::{clone_view, debug, pend, room, sign, unit};
use system_client::{Context, Pending};
use system_protocol::{
    Name, Path,
    control::{
        self,
        instance::{self as api, operations as op},
    },
    identity, loader, operator, supply,
    value::{Optional, Outcome, Rights},
};
use weave::{Codec, Contract, Operation};
const DEVICE: u64 = 0xbeef;
const READY: u64 = 0xabcd;
const CONTENTION_GRANTS: usize = 32;
static BUDGET: AtomicU64 = AtomicU64::new(0);
static CONTEND_SOURCE: AtomicU64 = AtomicU64::new(0);
static CONTEND_TARGET: AtomicU64 = AtomicU64::new(0);
static CONTEND_BATCHES: AtomicU64 = AtomicU64::new(0);
static CONTEND_BUSY: AtomicU64 = AtomicU64::new(0);
fn budget() -> u64 {
    BUDGET.load(Ordering::Relaxed)
}
#[global_allocator]
static ALLOCATOR: heap::Allocator = heap::Allocator;
struct Console;
impl fmt::Write for Console {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            let used = debug::put(VirtAddr::new(bytes.as_ptr().addr()), bytes.len().min(256))
                .map_err(|_| fmt::Error)?;
            bytes = &bytes[used..];
        }
        Ok(())
    }
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = writeln!(Console, "FAIL handoff {info}");
    termination::abort(71, "handoff: failure")
}
core::arch::global_asm!(
    ".section .text.entry,\"ax\"",
    ".global _start",
    "_start:",
    "tail start"
);
fn inspect(token: PendId) -> PendInfo {
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap();
    PendInfo::from_bytes(&bytes).unwrap()
}
fn find(predicate: impl Fn(&PendInfo) -> bool) -> PendInfo {
    let mut after = PendId::NONE;
    loop {
        let mut bytes = [0; PendInfo::SIZE];
        assert_eq!(
            pend::collect(after, VirtAddr::new(bytes.as_mut_ptr().addr()), 1).unwrap(),
            1
        );
        let info = PendInfo::from_bytes(&bytes).unwrap();
        if predicate(&info) {
            return info;
        }
        after = info.token;
    }
}
fn own() -> UnitInfo {
    let mut bytes = [0; UnitInfo::WIDTH];
    assert!(unit::inspect(UnitQuery::Caller, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap());
    UnitInfo::from_bytes(&bytes).unwrap()
}
#[track_caller]
fn tick(deadline: u64) {
    assert!(room::clock() < deadline, "handoff progress timeout");
    heap::poll();
    room::yield_();
}
fn query_tick(deadline: u64) {
    tick(deadline);
    // Query an asynchronous operation at a bounded cadence. Holding a reply
    // owner is useful; creating a new RPC on every yield only consumes the
    // same CPU and native memory budget needed by construction.
    let source = CONTEND_SOURCE.load(Ordering::Relaxed);
    if source == 0 || CONTEND_BATCHES.load(Ordering::Relaxed) != 0 {
        room::sleep_until(room::clock().saturating_add(500_000_000).min(deadline)).unwrap();
        return;
    }

    let target = TaskId::new(CONTEND_TARGET.load(Ordering::Relaxed) as usize);
    let source = PendId::from_bytes(&source.to_le_bytes()).unwrap();
    let bytes = AccordRequest {
        src: source,
        target: AccordTarget::Task(target),
        mark: Mark::of("handoff/auth-contention"),
        limits: Limits {
            operations: Operations::Clone(CloneRights::PEEK),
            modifiable: Modifiable::No,
            distributable: Distributable::No,
            scope: PendScope::Object,
        },
    }
    .to_bytes();

    for _ in 0..CONTENTION_GRANTS {
        let delivered = loop {
            match pend::accord(VirtAddr::new(bytes.as_ptr().addr())) {
                Ok(delivered) => break delivered,
                Err(error) if error.source.is_busy() => {
                    CONTEND_BUSY.fetch_add(1, Ordering::Relaxed);
                    tick(deadline);
                }
                Err(error) => panic!("authorization contention accord: {error:?}"),
            }
        };
        while let Err(error) = pend::revoke(target, delivered) {
            assert!(
                error.source.is_busy(),
                "authorization contention revoke: {error:?}"
            );
            CONTEND_BUSY.fetch_add(1, Ordering::Relaxed);
            tick(deadline);
        }
    }
    CONTEND_BATCHES.fetch_add(1, Ordering::Relaxed);
    room::sleep_until(room::clock().saturating_add(500_000_000).min(deadline)).unwrap();
}
fn retire(token: PendId) {
    let deadline = room::clock() + 5_000_000_000;
    while let Err(error) = pend::return_(token) {
        assert!(error.source.is_busy(), "{error:?}");
        tick(deadline);
    }
}
fn finish<O: Operation>(mut call: Pending<O>) -> <<O as Contract>::Response as Codec>::Decoded {
    let deadline = budget();
    // This User has no same-Team allocator workers. Use the SDK's native
    // reply watch instead of repeatedly polling Kernel while the peer runs.
    let value = call.wait().unwrap().value;
    while let Err(error) = call.finish() {
        assert!(error.is_busy(), "{error:?}");
        tick(deadline);
    }
    value
}
fn success<T, E: fmt::Debug>(value: Outcome<T, E>) -> T {
    match value {
        Outcome::Success(value) => value,
        Outcome::Failure(error) => panic!("service: {error:?}"),
    }
}
fn deadline() -> Wait {
    Wait::At(budget())
}
fn id(value: u64) -> api::OperationId {
    api::OperationId::new(value).unwrap()
}
fn receipt(client: &mut system_client::instance::Client, mut value: api::Receipt) -> api::Receipt {
    let limit = budget();
    while value.state == api::State::Pending {
        if room::clock() >= limit {
            let observation = value.instance.0.map(|instance| observe(client, instance));
            panic!(
                "operation {} did not complete: {observation:?}",
                value.id.get()
            );
        }
        query_tick(limit);
        value = success(finish(
            client
                .begin::<op::InspectOperation>(value.id, deadline())
                .unwrap(),
        ));
    }
    value
}
fn observe(
    client: &mut system_client::instance::Client,
    instance: api::InstanceId,
) -> api::Observation {
    success(finish(
        client.begin::<op::Inspect>(instance, deadline()).unwrap(),
    ))
}
fn clone_limits(distributable: Distributable) -> Limits {
    Limits {
        operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
        modifiable: Modifiable::Yes,
        distributable,
        scope: PendScope::Object,
    }
}
fn discovery(license: PendId, context: Context) -> system_client::operator::Client {
    let info = find(|info| info.mark == Mark::of("system/operator/discovery"));
    system_client::operator::Client::bind(info.token, info.owner, license, context).unwrap()
}
fn create(
    client: &mut system_client::instance::Client,
    subject: &identity::Subject,
    value: u64,
    authorization: bool,
) -> (api::InstanceId, api::Observation) {
    let request = api::Create {
        id: id(value),
        input: loader::Input::Unit(loader::UnitInput {
            name: Name::new("driver").unwrap(),
            stack: 512 * 1024,
            arguments: loader::Arguments {
                values: vec![if authorization { 1 } else { 5 }],
            },
        }),
        subject: subject.clone(),
    };
    let first = success(finish(
        client
            .begin::<op::Create>(request.clone(), deadline())
            .unwrap(),
    ));
    let target = first.instance.0.unwrap();
    if authorization {
        let repeated = success(finish(
            client
                .begin::<op::Create>(request.clone(), deadline())
                .unwrap(),
        ));
        assert_eq!(repeated.instance.0, Some(target));
        let mut changed = request;
        let loader::Input::Unit(ref mut input) = changed.input else {
            unreachable!()
        };
        input.arguments.values[0] = 2;
        assert_eq!(
            finish(client.begin::<op::Create>(changed, deadline()).unwrap()),
            Outcome::Failure(api::Fail::Conflict)
        );
    }
    let result = receipt(client, first);
    assert_eq!(result.state, api::State::Completed);
    let observed = observe(client, target);
    assert_eq!(observed.phase, api::Phase::Prepared);
    (target, observed)
}
fn bind_resource(
    discovery: &mut system_client::operator::Client,
    instance: api::InstanceId,
) -> operator::Resource {
    let associated = success(finish(
        discovery
            .begin::<operator::operations::ResolveRoot>(
                operator::Resolve {
                    mark: Mark::of("driver").into(),
                    name: Name::new("driver").unwrap(),
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(associated.instance.0, Some(instance));
    let entry = success(finish(
        discovery
            .begin::<operator::operations::Relative>(
                operator::Relative {
                    base: associated.pane.0.unwrap(),
                    path: Path::new("driver/rpc").unwrap(),
                },
                deadline(),
            )
            .unwrap(),
    ));
    success(finish(
        discovery
            .begin::<operator::operations::Find>(operator::At { entry }, deadline())
            .unwrap(),
    ))
}
fn ping(
    resource: &operator::Resource,
    target: &api::Observation,
    license: PendId,
    context: Context,
    revoked: bool,
) {
    let token = resource.token.try_into().unwrap();
    assert_eq!(inspect(token).owner.get() as u64, target.task.get());
    let response = query(
        resource,
        license,
        context,
        if revoked { "revoked" } else { "ping" },
    );
    assert_eq!(response.task, target.task);
    assert_eq!(response.team, target.team);
    assert_eq!(response.reason, u64::from(revoked));
}
fn query(
    resource: &operator::Resource,
    license: PendId,
    context: Context,
    name: &str,
) -> control::Observation {
    let token = resource.token.try_into().unwrap();
    let mut client =
        system_client::control::Client::bind(token, inspect(token).owner, license, context)
            .unwrap();
    success(finish(
        client
            .begin::<control::operations::State>(
                control::Named {
                    name: Name::new(name).unwrap(),
                },
                deadline(),
            )
            .unwrap(),
    ))
}

fn controller(license: PendId, context: Context, role: usize) {
    let endpoint = find(|info| info.mark == Mark::new(7));
    let mut client = system_client::instance::Client::bind(
        endpoint.token,
        endpoint.owner,
        license,
        context.clone(),
    )
    .unwrap();
    let info = own();
    let UnitTarget::Task(caller) = info.target else {
        panic!("Task")
    };
    let contention_root = if role == 6 {
        // This role's Instance endpoint is owned by the actual System Task.
        assert_ne!(endpoint.owner, caller, "System recipient identity");
        let root = clone_view::create(
            license,
            Mark::NONE,
            CloneLimits {
                max_len: 8,
                max_messages: 1,
                max_bytes: 8,
            },
        )
        .unwrap();
        CONTEND_SOURCE.store(root.get() as u64, Ordering::Relaxed);
        CONTEND_TARGET.store(endpoint.owner.get() as u64, Ordering::Relaxed);
        Some(root)
    } else {
        None
    };
    let resolver = find(|info| info.mark == Mark::of("system/identity/resolve"));
    let mut identity = system_client::identity::Client::bind(
        resolver.token,
        resolver.owner,
        license,
        context.clone(),
    )
    .unwrap();
    let subject = success(finish(
        identity
            .begin::<identity::operations::Resolve>(
                identity::Task {
                    task: caller.into(),
                },
                deadline(),
            )
            .unwrap(),
    ))
    .0
    .unwrap()
    .current;
    if role == 2 {
        let accepted = success(finish(
            client
                .begin::<op::Create>(
                    api::Create {
                        id: id(1),
                        input: loader::Input::Unit(loader::UnitInput {
                            name: Name::new("driver").unwrap(),
                            stack: 512 * 1024,
                            arguments: loader::Arguments { values: vec![1] },
                        }),
                        subject,
                    },
                    deadline(),
                )
                .unwrap(),
        ));
        assert!(accepted.instance.0.is_some());
        let _ = writeln!(Console, "PASS handoff owner-death accepted=1");
        return;
    }
    if role == 4 {
        cancellation(&mut client, &subject);
        return;
    }
    let mut discovery = discovery(license, context.clone());
    if role == 3 {
        let limit = budget();
        let target = loop {
            if let Outcome::Success(row) = finish(
                discovery
                    .begin::<operator::operations::ResolveRoot>(
                        operator::Resolve {
                            mark: Mark::of("driver").into(),
                            name: Name::new("driver").unwrap(),
                        },
                        deadline(),
                    )
                    .unwrap(),
            ) && let Some(target) = row.instance.0
            {
                break target;
            }
            // The observer only needs one published association. Give ELF
            // construction CPU time rather than rediscovering it twice/second.
            tick(limit);
            room::sleep_until(room::clock().saturating_add(2_000_000_000).min(limit)).unwrap();
        };
        assert_eq!(
            finish(client.begin::<op::Inspect>(target, deadline()).unwrap()),
            Outcome::Failure(api::Fail::Denied)
        );
        let denied = success(finish(
            client
                .begin::<op::Stop>(
                    api::Change {
                        id: id(1),
                        instance: target,
                    },
                    deadline(),
                )
                .unwrap(),
        ));
        assert_eq!(denied.state, api::State::Failed);
        assert_eq!(denied.failure.0.unwrap().0, api::Fail::Denied);
        let resource = bind_resource(&mut discovery, target);
        assert_eq!(
            query(&resource, license, context.clone(), "observer").reason,
            1
        );
        retire(resource.token.try_into().unwrap());
        let _ = writeln!(Console, "PASS handoff authorization=1");
        return;
    }
    let authorization = role == 6;
    if authorization {
        let absent = success(finish(
            discovery
                .begin::<operator::operations::OwnRoot>(
                    operator::unit::After { cursor: 0 },
                    deadline(),
                )
                .unwrap(),
        ));
        assert!(absent.0.is_none());
    }
    let rounds = if authorization { 1 } else { 2 };
    let signal = sign::create(license).unwrap();
    let mut source = source::Source::new("handoff").unwrap();
    let mut previous = None;
    for round in 0..rounds {
        let _ = writeln!(
            Console,
            "handoff: round {round} start t={}ms",
            room::clock() / 1_000_000
        );
        let base = 1 + round * 10;
        let (target, observed) = create(&mut client, &subject, base, authorization);
        if authorization {
            // Cancellation cannot undo an already completed Create receipt.
            let cancellation = success(finish(
                client.begin::<op::Cancel>(id(base), deadline()).unwrap(),
            ));
            assert_eq!(cancellation.state, api::State::Completed);
        }

        if let Some((old, _, _)) = &previous {
            assert_ne!(*old, target);
        }
        assert_ne!(observed.team.get(), info.team.get() as u64);
        assert_ne!(observed.task.get(), caller.get() as u64);
        let actual = TaskId::new(observed.task.get() as usize);
        let maximum = Limits {
            operations: Operations::Sign(SignRights::TRIGGER),
            modifiable: Modifiable::No,
            distributable: Distributable::No,
            scope: PendScope::Object,
        };
        let key = alloc::format!("signal{round}");
        source
            .register(source::Resource {
                name: &key,
                source: signal,
                maximum,
                caller,
                target: Some(actual),
                description: b"cpu signal",
            })
            .unwrap();
        let acquisition = supply::Acquire {
            lease: supply::LeaseId::new(base).unwrap(),
            target: observed.task,
            binding: supply::Binding {
                source: supply::Key {
                    name: String::from("handoff"),
                },
                resource: supply::Key { name: key },
                slot: system_protocol::Mark::new(DEVICE),
                rights: Rights(maximum),
            },
        };
        source.acquire(caller, acquisition.clone()).unwrap();
        let limit = budget();
        loop {
            source.advance().unwrap();
            if source.inspect(caller, acquisition.lease).unwrap().state == supply::State::Granted {
                break;
            }
            tick(limit);
        }
        let delivery = source
            .inspect(caller, acquisition.lease)
            .unwrap()
            .delivery
            .0
            .unwrap();
        assert_ne!(delivery.token, system_protocol::PendId::NONE);
        let started = success(finish(
            client
                .begin::<op::Start>(
                    api::Change {
                        id: id(base + 1),
                        instance: target,
                    },
                    deadline(),
                )
                .unwrap(),
        ));
        assert_eq!(receipt(&mut client, started).state, api::State::Completed);
        let observed = loop {
            let value = observe(&mut client, target);
            if value.ready {
                break value;
            }
            query_tick(limit);
        };
        let root = observed.root.0.as_ref().unwrap();
        let kernel_api::abi::call::sign::SignReply::Triggered { sequence } =
            sign::listen(signal, Wait::Poll).unwrap()
        else {
            panic!("missing real supplied signal");
        };
        sign::consume(signal, sequence).unwrap();
        assert_eq!(root.registration.instance, target);
        assert_eq!(root.registration.owner.get(), caller.get() as u64);
        assert_eq!(root.phase, operator::unit::Phase::Active);
        let revision = if round == 0 { 0 } else { 1 };
        let publication = api::Publication {
            id: id(base + 2),
            name: Name::new("driver").unwrap(),
            expected: revision,
            instance: Optional(Some(target)),
        };
        let published = success(finish(
            client
                .begin::<op::Publish>(publication.clone(), deadline())
                .unwrap(),
        ));
        assert_eq!(published.state, api::State::Completed);
        if authorization {
            assert_eq!(
                success(finish(
                    client
                        .begin::<op::Publish>(publication.clone(), deadline())
                        .unwrap()
                )),
                published
            );
            let stale = success(finish(
                client
                    .begin::<op::Publish>(
                        api::Publication {
                            id: id(40),
                            ..publication
                        },
                        deadline(),
                    )
                    .unwrap(),
            ));
            assert_eq!(stale.state, api::State::Failed);
            assert_eq!(stale.failure.0.unwrap().0, api::Fail::Conflict);
        }
        let resource = bind_resource(&mut discovery, target);
        ping(&resource, &observed, license, context.clone(), false);
        if authorization {
            let limit = budget();
            while query(&resource, license, context.clone(), "authorization").reason != 1 {
                query_tick(limit);
            }
        }
        if let Some((old, old_resource, old_observed)) = previous.take() {
            // Rebinding discovery preserves the first consumer's actual Pend.
            ping(
                &old_resource,
                &old_observed,
                license,
                context.clone(),
                false,
            );
            retire(old_resource.token.try_into().unwrap());
            let stop = success(finish(
                client
                    .begin::<op::Stop>(
                        api::Change {
                            id: id(base + 3),
                            instance: old,
                        },
                        deadline(),
                    )
                    .unwrap(),
            ));
            assert_eq!(receipt(&mut client, stop).state, api::State::Completed);
            let retired = success(finish(
                client
                    .begin::<op::Retire>(
                        api::Change {
                            id: id(base + 4),
                            instance: old,
                        },
                        deadline(),
                    )
                    .unwrap(),
            ));
            assert_eq!(receipt(&mut client, retired).state, api::State::Completed);
            assert_eq!(
                observe(&mut client, old).root.0.unwrap().phase,
                operator::unit::Phase::Retired
            );
            let still_current = success(finish(
                discovery
                    .begin::<operator::operations::ResolveRoot>(
                        operator::Resolve {
                            mark: Mark::of("driver").into(),
                            name: Name::new("driver").unwrap(),
                        },
                        deadline(),
                    )
                    .unwrap(),
            ));
            assert_eq!(still_current.instance.0, Some(target));
            // Snapshots supply expected identity only; native Pend ownership
            // and actual RPC sender are checked again by ping.
            ping(&resource, &observed, license, context.clone(), false);
        }
        previous = Some((target, resource, observed));
        let _ = writeln!(Console, "handoff: round {round} complete");
    }
    let (last, resource, last_observed) = previous.take().unwrap();
    // Lease withdrawal is independent of Task exit and namespace discovery.
    let last_lease = supply::LeaseId::new(1 + (rounds - 1) * 10).unwrap();
    source.release(caller, last_lease).unwrap();
    let limit = room::clock() + 5_000_000_000;
    while source.inspect(caller, last_lease).unwrap().state != supply::State::Released {
        source.advance().unwrap();
        tick(limit);
    }
    let _ = writeln!(Console, "handoff: lease withdrawn");
    ping(&resource, &last_observed, license, context.clone(), true);
    retire(resource.token.try_into().unwrap());
    let retired = success(finish(
        client
            .begin::<op::Retire>(
                api::Change {
                    id: id(30),
                    instance: last,
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(receipt(&mut client, retired).state, api::State::Completed);
    assert_eq!(observe(&mut client, last).phase, api::Phase::Retired);
    if authorization {
        assert_eq!(
            finish(
                client
                    .begin::<op::Inspect>(
                        api::InstanceId {
                            system: system_protocol::TaskId::new(999),
                            ..last
                        },
                        deadline()
                    )
                    .unwrap()
            ),
            Outcome::Failure(api::Fail::Unknown)
        );
    }
    let cleanup_limit = room::clock() + 5_000_000_000;
    while !source.finish().unwrap() {
        tick(cleanup_limit);
    }
    if role == 6 {
        assert_eq!(
            CONTEND_BATCHES.load(Ordering::Relaxed),
            1,
            "authorization Pending path must execute exactly one contention batch"
        );
        let _ = writeln!(
            Console,
            "handoff: contention pairs={} busy={}",
            CONTENTION_GRANTS,
            CONTEND_BUSY.load(Ordering::Relaxed)
        );
        CONTEND_SOURCE.store(0, Ordering::Relaxed);
        CONTEND_TARGET.store(0, Ordering::Relaxed);
        retire(contention_root.expect("authorization contention source"));
    }
    retire(signal);
    let _ = writeln!(
        Console,
        "{}",
        if authorization {
            "PASS handoff create=1 replay=1 conflict=1 root-grants=1 stale=1"
        } else {
            "PASS handoff recovery=1 instance=1 roots=1 cas=1 rpc=1 supply=1 revoke=1 stop=1 retire=1"
        }
    );
}
fn cancellation(client: &mut system_client::instance::Client, subject: &identity::Subject) {
    // Ignore the first application response after admission. Querying the same
    // operation recovers its identity without dispatching Create a second time.
    let pending_request = api::Create {
        id: id(50),
        input: loader::Input::Unit(loader::UnitInput {
            name: Name::new("driver").unwrap(),
            stack: 512 * 1024,
            arguments: loader::Arguments { values: vec![1] },
        }),
        subject: subject.clone(),
    };
    let mut lost = client
        .begin::<op::Create>(pending_request.clone(), deadline())
        .unwrap();
    // FIFO submission places cancellation immediately after admission, before
    // another query/cleanup round can let the native construction finish.
    let cancel = success(finish(
        client.begin::<op::Cancel>(id(50), deadline()).unwrap(),
    ));
    let accepted = success(finish(
        client
            .begin::<op::InspectOperation>(id(50), deadline())
            .unwrap(),
    ));
    assert!(matches!(
        accepted.state,
        api::State::Pending | api::State::Cancelled
    ));
    let lost_limit = room::clock() + 5_000_000_000;
    while let Err(error) = lost.finish() {
        assert!(error.is_busy());
        tick(lost_limit);
    }
    let cancelled = receipt(client, cancel);
    assert_eq!(cancelled.state, api::State::Cancelled);
    assert_eq!(
        observe(client, accepted.instance.0.unwrap()).phase,
        api::Phase::Retired
    );
    assert_eq!(
        success(finish(
            client
                .begin::<op::Create>(pending_request, deadline())
                .unwrap()
        )),
        cancelled
    );
    let _ = writeln!(Console, "PASS handoff lost-reply=1 pending-cancel=1");
}

fn driver(
    license: PendId,
    context: Context,
    authorization: bool,
    installed: bool,
    dynamic: bool,
) -> ! {
    let info = own();
    let UnitTarget::Task(caller) = info.target else {
        panic!("Task")
    };
    let device = find(|info| info.mark == Mark::new(DEVICE));
    assert_eq!(
        device.limits.operations,
        Operations::Sign(SignRights::TRIGGER)
    );
    assert_eq!(device.limits.distributable, Distributable::No);
    if installed || dynamic {
        assert_eq!(
            device.lender, info.creator,
            "System performs declared installation"
        );
    }
    let mut supplied = if installed {
        let info = find(|info| info.mark == Mark::new(9));
        Some(
            system_client::supply::Client::bind(info.token, info.owner, license, context.clone())
                .unwrap(),
        )
    } else {
        None
    };
    let second = dynamic.then(|| find(|info| info.mark == Mark::new(DEVICE + 1)).token);
    let third = dynamic.then(|| find(|info| info.mark == Mark::new(DEVICE + 2)).token);
    let mut lease = 1;
    if let Some(client) = supplied.as_mut() {
        provider::self_lease(client, caller, lease, false);
        lease += 1;
    }
    sign::trigger(device.token).unwrap();
    let mut discovery = discovery(license, context.clone());
    let access = success(finish(
        discovery
            .begin::<operator::operations::OwnRoot>(operator::unit::After { cursor: 0 }, deadline())
            .unwrap(),
    ))
    .0
    .unwrap();
    assert_eq!(access.root.registration.task.get(), caller.get() as u64);
    assert_eq!(access.root.phase, operator::unit::Phase::Preparing);
    if dynamic {
        install::foreign(
            license,
            context.clone(),
            access.root.registration.instance,
            caller,
        );
    }
    let grant = &access.grant;
    assert_eq!(grant.spec.holder.get(), caller.get() as u64);
    assert_eq!(grant.spec.source.0, Some(caller.into()));
    if authorization {
        assert!(
            success(finish(
                discovery
                    .begin::<operator::operations::OwnRoot>(
                        operator::unit::After {
                            cursor: access.cursor
                        },
                        deadline()
                    )
                    .unwrap()
            ))
            .0
            .is_none()
        );
    }
    let entry: PendId = grant.entrance.try_into().unwrap();
    assert_eq!(
        inspect(entry).owner.get() as u64,
        access.root.registration.pane.entry.operator.get()
    );
    let operator = inspect(entry).owner;
    let mut authority =
        system_client::authority::Client::bind(entry, operator, license, context.clone()).unwrap();
    if authorization {
        assert_eq!(
            finish(
                authority
                    .begin::<operator::authority::operations::RegisterRoot>(
                        access.root.registration.clone(),
                        deadline()
                    )
                    .unwrap()
            ),
            Outcome::Failure(operator::Fail::Denied)
        );
        assert_eq!(
            finish(
                authority
                    .begin::<operator::authority::operations::AdvanceRoot>(
                        operator::unit::Advance {
                            instance: access.root.registration.instance,
                            expected: access.root.revision,
                            phase: operator::unit::Phase::Active,
                        },
                        deadline()
                    )
                    .unwrap()
            ),
            Outcome::Failure(operator::Fail::Denied)
        );
    }
    let bytes = <control::Call as Contract>::Request::SIZE
        .maximum()
        .unwrap();
    let queue = clone_view::create(
        license,
        Mark::of("handoff-driver"),
        CloneLimits {
            max_len: bytes,
            max_messages: 16,
            max_bytes: 16 * bytes,
        },
    )
    .unwrap();
    let publication = AccordRequest {
        src: queue,
        target: AccordTarget::Task(operator),
        mark: Mark::of("handoff-driver"),
        limits: clone_limits(Distributable::Yes),
    }
    .to_bytes();
    let remote = loop {
        match pend::accord(VirtAddr::new(publication.as_ptr().addr())) {
            Ok(remote) => break remote,
            Err(error) if error.source.is_busy() => tick(budget()),
            Err(error) => panic!("Driver RPC grant publication: {error:?}"),
        }
    };
    success(finish(
        authority
            .begin::<operator::authority::operations::Bind>(
                operator::Bind {
                    change: operator::Mutation {
                        id: operator::OperationId::new(1).unwrap(),
                        entry: grant.spec.anchor.clone(),
                        expected: 0,
                    },
                    name: Name::new("rpc").unwrap(),
                    resource: remote.into(),
                    resource_id: 1,
                    rights: Rights(clone_limits(Distributable::No)),
                    permit: operator::Permit::Public,
                    source: operator::Source {
                        task: caller.into(),
                        mark: Mark::of("handoff-driver").into(),
                    },
                },
                deadline(),
            )
            .unwrap(),
    ));
    let mut ingress = protocol::Control::bind(queue, license, context).unwrap();
    sign::trigger(find(|info| info.mark == Mark::new(READY)).token).unwrap();
    let mut observer = false;
    loop {
        let result = ingress.serve_once(Wait::At(budget()), |_, request| match request {
            control::operations::DecodedRequest::State(request) => {
                let name = request.name.as_str();
                if name == "observer" {
                    observer = true;
                }
                let reason = if dynamic && name == "second" {
                    sign::trigger(second.unwrap()).unwrap();
                    2
                } else if dynamic && name == "third" {
                    sign::trigger(third.unwrap()).unwrap();
                    3
                } else if dynamic && name == "revoked-second" {
                    let error =
                        sign::trigger(second.unwrap()).expect_err("withdrawn second installation");
                    assert!(!error.source.is_busy());
                    4
                } else if name == "acquire" && installed {
                    provider::self_lease(supplied.as_mut().unwrap(), caller, lease, true);
                    lease += 1;
                    2
                } else if matches!(name, "observer" | "authorization") {
                    u64::from(observer)
                } else {
                    u64::from(name == "revoked")
                };
                let revoked = name == "revoked";
                if revoked {
                    let error =
                        sign::trigger(device.token).expect_err("withdrawn lease remained usable");
                    assert!(!error.source.is_busy(), "Busy is not revocation evidence");
                }
                control::operations::Response::State(Outcome::Success(control::Observation {
                    state: control::State::Ready,
                    reason,
                    completed: false,
                    task: caller.into(),
                    team: info.team.into(),
                }))
            }
            _ => panic!("unexpected driver request"),
        });
        assert!(matches!(result, Ok(()) | Err(protocol::Error::NotReady)));
        heap::poll();
        room::yield_();
    }
}
#[unsafe(no_mangle)]
extern "C" fn start(words: *const usize, count: usize) -> ! {
    // The workflow is asynchronous and includes native ELF preparation and
    // independent service startup. Bound the whole scenario separately from
    // an individual request wait; keep each original Pending/operation owner.
    let seconds = option_env!("SHUYRA_HANDOFF_BUDGET_SECONDS")
        .unwrap_or("60")
        .parse::<u64>()
        .expect("handoff budget");
    assert!(matches!(seconds, 60 | 120));
    BUDGET.store(
        room::clock().saturating_add(seconds * 1_000_000_000),
        Ordering::Relaxed,
    );
    assert_eq!(count, 1);
    let license = find(|info| info.target == Target::None).token;
    let space = find(|info| matches!(info.target, Target::Nest(_))).token;
    // SAFETY: Loader delivers this Team's actual ChildNest and creation grant.
    unsafe { heap::initialize(license, space) }.unwrap();
    let Target::Nest(nest) = inspect(space).target else {
        panic!("Nest")
    };
    let context = Context::new(unsafe {
        twine::view::memory::Native::new(space, nest, memory::next_request as fn() -> _)
    });
    // SAFETY: Kernel supplies one copied word on this Task's live stack.
    let role = unsafe { *words };
    if role == 9 {
        owner::driver(license, context)
    }
    if matches!(role, 7 | 8 | 10 | 11 | 12 | 13) {
        owner::run(license, context, role);
        unit::exit(17, 0, VirtAddr::new(0), 0, ExitScope::Task)
    }
    if matches!(role, 16 | 18) {
        install::run(license, context, role == 18);
        unit::exit(17, 0, VirtAddr::new(0), 0, ExitScope::Task)
    }
    if role == 14 {
        provider::run(license, context);
        unit::exit(17, 0, VirtAddr::new(0), 0, ExitScope::Task)
    }
    if matches!(role, 1 | 5 | 15 | 17) {
        driver(license, context, role == 1, role == 15, role == 17)
    }
    controller(license, context, role);
    unit::exit(17, 0, VirtAddr::new(0), 0, ExitScope::Task)
}

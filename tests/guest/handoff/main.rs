//! Independent ordinary User Teams consuming the production System handoff.
#![no_std]
#![no_main]
extern crate alloc;
#[path = "../../../server/src/runtime/heap/mod.rs"]
pub mod heap;
#[path = "../../../server/src/runtime/memory.rs"]
pub mod memory;
#[path = "../../../server/src/runtime/termination.rs"]
pub mod termination;
pub mod runtime {
    pub use crate::{memory, termination};
}
#[path = "../../../server/src/protocol/mod.rs"]
pub mod protocol;
#[path = "../../../server/src/system/supply/native.rs"]
pub mod source;
use alloc::{string::String, vec};
use core::fmt::{self, Write};
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
fn retire(token: PendId) {
    let deadline = room::clock() + 5_000_000_000;
    while let Err(error) = pend::return_(token) {
        assert!(error.source.is_busy(), "{error:?}");
        tick(deadline);
    }
}
fn finish<O: Operation>(mut call: Pending<O>) -> <<O as Contract>::Response as Codec>::Decoded {
    let deadline = room::clock() + 5_000_000_000;
    let value = loop {
        if let Some(response) = call.poll().unwrap() {
            break response.value;
        }
        tick(deadline);
    };
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
    Wait::At(room::clock() + 5_000_000_000)
}
fn id(value: u64) -> api::OperationId {
    api::OperationId::new(value).unwrap()
}
fn receipt(client: &mut system_client::instance::Client, mut value: api::Receipt) -> api::Receipt {
    let limit = room::clock() + 5_000_000_000;
    while value.state == api::State::Pending {
        if room::clock() >= limit {
            let observation = value.instance.0.map(|instance| observe(client, instance));
            panic!("operation {} did not complete: {observation:?}", value.id.get());
        }
        tick(limit);
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
) -> api::InstanceId {
    let request = api::Create {
        id: id(value),
        input: loader::Input::Unit(loader::UnitInput {
            name: Name::new("driver").unwrap(),
            stack: 512 * 1024,
            arguments: loader::Arguments { values: vec![1] },
        }),
        subject: subject.clone(),
    };
    let first = success(finish(
        client
            .begin::<op::Create>(request.clone(), deadline())
            .unwrap(),
    ));
    let target = first.instance.0.unwrap();
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
    let result = receipt(client, first);
    assert_eq!(result.state, api::State::Completed);
    assert_eq!(observe(client, target).phase, api::Phase::Prepared);
    target
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
    let mut client =
        system_client::control::Client::bind(token, inspect(token).owner, license, context)
            .unwrap();
    let response = success(finish(
        client
            .begin::<control::operations::State>(
                control::Named {
                    name: Name::new(if revoked { "revoked" } else { "ping" }).unwrap(),
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(response.task, target.task);
    assert_eq!(response.team, target.team);
    assert_eq!(response.reason, u64::from(revoked));
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
    let mut discovery = discovery(license, context.clone());
    if role == 3 {
        let limit = room::clock() + 5_000_000_000;
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
            tick(limit);
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
        let _ = writeln!(Console, "PASS handoff authorization=1");
        return;
    }
    let absent = success(finish(
        discovery
            .begin::<operator::operations::OwnRoot>((), deadline())
            .unwrap(),
    ));
    assert!(absent.0.is_none());
    let signal = sign::create(license).unwrap();
    let mut source = source::Source::new("handoff").unwrap();
    let mut previous = None;
    for round in 0..2 {
        let base = 1 + round * 10;
        let target = create(&mut client, &subject, base);
        if let Some((old, _)) = &previous {
            assert_ne!(*old, target);
        }
        let observed = observe(&mut client, target);
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
        let limit = room::clock() + 5_000_000_000;
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
            tick(limit);
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
        assert_eq!(
            success(finish(
                client
                    .begin::<op::Publish>(publication, deadline())
                    .unwrap()
            )),
            published
        );
        let resource = bind_resource(&mut discovery, target);
        ping(&resource, &observed, license, context.clone(), false);
        if let Some((old, old_resource)) = previous.take() {
            // Rebinding discovery preserves the first consumer's actual Pend.
            ping(
                &old_resource,
                &observe(&mut client, old),
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
            let still_current = bind_resource(&mut discovery, target);
            assert_eq!(still_current.binding, resource.binding);
            retire(still_current.token.try_into().unwrap());
        }
        previous = Some((target, resource));
        let _ = writeln!(Console, "handoff: round {round} complete");
    }
    let (last, resource) = previous.take().unwrap();
    // Lease withdrawal is independent of Task exit and namespace discovery.
    source
        .release(caller, supply::LeaseId::new(11).unwrap())
        .unwrap();
    let limit = room::clock() + 5_000_000_000;
    while source
        .inspect(caller, supply::LeaseId::new(11).unwrap())
        .unwrap()
        .state
        != supply::State::Released
    {
        source.advance().unwrap();
        tick(limit);
    }
    let _ = writeln!(Console, "handoff: lease withdrawn");
    ping(
        &resource,
        &observe(&mut client, last),
        license,
        context.clone(),
        true,
    );
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
    let cancelled = create(&mut client, &subject, 40);
    let cancellation = success(finish(
        client.begin::<op::Cancel>(id(40), deadline()).unwrap(),
    ));
    // Create already completed; cancellation cannot undo its historical receipt.
    assert_eq!(cancellation.state, api::State::Completed);
    let retired = success(finish(
        client
            .begin::<op::Retire>(
                api::Change {
                    id: id(41),
                    instance: cancelled,
                },
                deadline(),
            )
            .unwrap(),
    ));
    assert_eq!(receipt(&mut client, retired).state, api::State::Completed);
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
    let accepted = success(finish(
        client
            .begin::<op::InspectOperation>(id(50), deadline())
            .unwrap(),
    ));
    assert_eq!(accepted.state, api::State::Pending);
    let lost_limit = room::clock() + 5_000_000_000;
    while let Err(error) = lost.finish() {
        assert!(error.is_busy());
        tick(lost_limit);
    }
    let cancel = success(finish(
        client.begin::<op::Cancel>(id(50), deadline()).unwrap(),
    ));
    let cancelled = receipt(&mut client, cancel);
    assert_eq!(cancelled.state, api::State::Cancelled);
    assert_eq!(
        observe(&mut client, accepted.instance.0.unwrap()).phase,
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
    let cleanup_limit = room::clock() + 5_000_000_000;
    while !source.finish().unwrap() {
        tick(cleanup_limit);
    }
    retire(signal);
    let _ = writeln!(
        Console,
        "PASS handoff create=1 replay=1 conflict=1 instance=1 roots=1 cas=1 rpc=1 supply=1 revoke=1 stop=1 retire=1"
    );
}
fn driver(license: PendId, context: Context) -> ! {
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
    sign::trigger(device.token).unwrap();
    let mut discovery = discovery(license, context.clone());
    let access = success(finish(
        discovery
            .begin::<operator::operations::OwnRoot>((), deadline())
            .unwrap(),
    ))
    .0
    .unwrap();
    assert_eq!(access.root.registration.task.get(), caller.get() as u64);
    assert_eq!(access.root.phase, operator::unit::Phase::Preparing);
    assert_eq!(access.grants.len(), 1);
    let grant = &access.grants[0];
    let entry: PendId = grant.entrance.try_into().unwrap();
    assert_eq!(
        inspect(entry).owner.get() as u64,
        access.root.registration.pane.entry.operator.get()
    );
    let operator = inspect(entry).owner;
    let mut authority =
        system_client::authority::Client::bind(entry, operator, license, context.clone()).unwrap();
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
    let remote = pend::accord(VirtAddr::new(
        AccordRequest {
            src: queue,
            target: AccordTarget::Task(operator),
            mark: Mark::of("handoff-driver"),
            limits: clone_limits(Distributable::Yes),
        }
        .to_bytes()
        .as_ptr()
        .addr(),
    ))
    .unwrap();
    success(finish(
        authority
            .begin::<operator::authority::operations::Bind>(
                operator::Bind {
                    change: operator::Mutation {
                        id: operator::OperationId::new(1).unwrap(),
                        entry: grant.anchor.clone(),
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
    loop {
        let result = ingress.serve_once(Wait::Poll, |_, request| match request {
            control::operations::DecodedRequest::State(request) => {
                let revoked = request.name.as_str() == "revoked";
                if revoked {
                    let error =
                        sign::trigger(device.token).expect_err("withdrawn lease remained usable");
                    assert!(!error.source.is_busy(), "Busy is not revocation evidence");
                }
                control::operations::Response::State(Outcome::Success(control::Observation {
                    state: control::State::Ready,
                    reason: u64::from(revoked),
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
    if role == 1 {
        driver(license, context)
    }
    controller(license, context, role);
    unit::exit(17, 0, VirtAddr::new(0), 0, ExitScope::Task)
}

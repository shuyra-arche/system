//! Actual Client/Ingress replies share the heap's native request allocator.
use crate::{addr, heap, memory, protocol, worker::Worker};
use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering::*};
use kernel_api::{
    address::VirtAddr,
    model::{
        clone_view::CloneLimits,
        identity::{Mark, PendId, TaskId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, CloneRights, CreationRights, Distributable, Limits,
            NestRights, NestScope, Operations, PendInfo, PendScope, Target,
        },
        unit::{ExitScope, UnitQuery, UnitTarget},
        wait::Wait,
    },
};
use kernel_client::call::{clone_view, pend, room, unit};
use system_client::{Context, control::Client};
use system_protocol::{
    Name,
    control::{self as api, operations as op},
    value::Outcome,
};
use twine::view::memory::Native;

static CALLER: AtomicUsize = AtomicUsize::new(0);
static LICENSE: AtomicUsize = AtomicUsize::new(0);
static SPACE: AtomicUsize = AtomicUsize::new(0);
static ENTRY: AtomicUsize = AtomicUsize::new(0);
fn native(value: usize) -> PendId {
    PendId::from_bytes(&(value as u64).to_le_bytes()).unwrap()
}
fn context(space: PendId) -> Context {
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(space, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap();
    let Target::Nest(nest) = PendInfo::from_bytes(&bytes).unwrap().target else {
        panic!("Nest")
    };
    // SAFETY: run explicitly delivers the same Team's Internal Nest to this
    // Task. Every native-memory caller shares the production request allocator.
    Context::new(unsafe { Native::new(space, nest, memory::next_request as fn() -> _) })
}
fn grant(
    source: PendId,
    target: TaskId,
    operations: Operations,
    scope: PendScope,
    modifiable: Modifiable,
) -> PendId {
    pend::accord(addr(
        &AccordRequest {
            src: source,
            target: AccordTarget::Task(target),
            mark: Mark::NONE,
            limits: Limits {
                operations,
                scope,
                modifiable,
                distributable: Distributable::No,
            },
        }
        .to_bytes(),
    ))
    .unwrap()
}
fn tick(deadline: u64) {
    assert!(room::clock() < deadline, "RPC progress deadline");
    heap::poll();
    room::yield_();
}
pub fn run(license: PendId, space: PendId) {
    let UnitTarget::Task(caller) = crate::inspect(UnitQuery::Caller).target else {
        panic!("Task")
    };
    CALLER.store(caller.get(), Release);
    // SAFETY: the worker receives only static atomics and explicit grants. Its
    // administrative owner is retained until Kernel reclamation and deletion.
    let mut worker = unsafe { Worker::prepare(serve, &[], 256 * 1024) }.unwrap();
    LICENSE.store(
        grant(
            license,
            worker.task(),
            Operations::None(CreationRights::CLONE | CreationRights::SIGN),
            PendScope::Object,
            Modifiable::No,
        )
        .get(),
        Release,
    );
    SPACE.store(
        grant(
            space,
            worker.task(),
            Operations::Nest(NestRights::INSPECT | NestRights::RESERVE),
            PendScope::Nest(NestScope::Whole),
            Modifiable::Yes,
        )
        .get(),
        Release,
    );
    worker.resume().unwrap();
    let deadline = room::clock() + 10_000_000_000;
    while ENTRY.load(Acquire) == 0 {
        tick(deadline);
    }
    let mut client = Client::bind(
        native(ENTRY.load(Acquire)),
        worker.task(),
        license,
        context(space),
    )
    .unwrap();
    for _ in 0..2 {
        let mut pending = client
            .begin::<op::State>(
                api::Named {
                    name: Name::new("probe").unwrap(),
                },
                Wait::At(deadline),
            )
            .unwrap();
        let response = loop {
            if let Some(response) = pending.poll().unwrap() {
                break response;
            }
            tick(deadline);
        };
        assert_eq!(response.metadata, worker.task());
        let Outcome::Success(observation) = response.value else {
            panic!("response")
        };
        assert_eq!(observation.state, api::State::Ready);
        while let Err(error) = pending.finish() {
            assert!(error.is_busy(), "{error:?}");
            tick(deadline);
        }
    }
    loop {
        heap::poll();
        match worker.finish() {
            Ok(true) => break,
            Ok(false) => tick(deadline),
            Err(error) => panic!("worker: {error:?}"),
        }
    }
    let result = worker.result().unwrap();
    assert!(result.reclaimed);
    assert_eq!(result.result, Some(31));
    writeln!(
        crate::Console,
        "PASS system-rpc abi=11 protocol=4 context=1 sender=1 reply=1 cleanup=1 worker-reclaimed=1"
    )
    .unwrap();
}
unsafe extern "C" fn serve(_: *const usize, count: usize) -> ! {
    assert_eq!(count, 0);
    let license = native(LICENSE.load(Acquire));
    let caller = TaskId::new(CALLER.load(Acquire));
    let queue = clone_view::create(
        license,
        Mark::NONE,
        CloneLimits {
            max_len: 1024,
            max_messages: 4,
            max_bytes: 4096,
        },
    )
    .unwrap();
    let mut endpoint =
        protocol::Control::bind(queue, license, context(native(SPACE.load(Acquire)))).unwrap();
    let remote = grant(
        queue,
        caller,
        Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
        PendScope::Object,
        Modifiable::Yes,
    );
    ENTRY.store(remote.get(), Release);
    for _ in 0..2 {
        endpoint
            .serve_once(
                Wait::At(room::clock() + 10_000_000_000),
                |actual, request| {
                    assert_eq!(actual, caller);
                    let op::DecodedRequest::State(request) = request else {
                        panic!("operation")
                    };
                    assert_eq!(request.name.as_str(), "probe");
                    op::Response::State(Outcome::Success(api::Observation {
                        state: api::State::Ready,
                        reason: 0,
                        completed: false,
                        task: system_protocol::TaskId::NONE,
                        team: system_protocol::TeamId::NONE,
                    }))
                },
            )
            .unwrap();
    }
    endpoint.finish().unwrap();
    pend::return_(queue).unwrap();
    unit::exit(31, 0, VirtAddr::new(0), 0, ExitScope::Task)
}

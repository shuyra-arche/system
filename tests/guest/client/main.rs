#![no_std]
#![no_main]

extern crate alloc;

mod wait_group;
#[path = "../../../server/src/system/app/waiting/mod.rs"]
mod waiting;

mod actor;
mod adjacency;
mod authority_life;
mod authority_query;
mod fixture;
mod operator;
#[allow(dead_code)]
#[path = "../../../server/src/system/operator/authority.rs"]
pub mod operator_authority;
use operator_tree::gate as operator_gate;
#[path = "../../../server/src/system/operator/runtime/mod.rs"]
pub mod operator_runtime;
#[path = "../../../server/src/system/operator/tree/mod.rs"]
pub mod operator_tree;
#[path = "../../../server/src/system/operator/watch.rs"]
pub mod operator_watch;
use identity_service::answer as identity_answer;
#[allow(dead_code)]
#[path = "../../../server/src/system/identity/book/mod.rs"]
pub mod identity_book;
#[path = "../../../server/src/system/identity/service/mod.rs"]
pub mod identity_service;
#[allow(dead_code)]
#[path = "../../../server/src/protocol/mod.rs"]
mod protocol;
#[path = "../../../server/src/system/identity/revision.rs"]
pub mod revision;
pub mod system {
    pub mod operator {
        pub use crate::{
            operator_authority as authority, operator_runtime as runtime, operator_tree as tree,
            operator_watch as watch,
        };
    }
    pub(crate) mod app {
        pub(crate) use crate::waiting;
    }
    pub mod identity {
        pub use crate::identity_book as book;
    }
}

use core::{
    fmt::Write as _,
    sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
use fixture::{Console, deadline, exit, until};
use kernel_api::model::{
    clone_view::CloneLimits,
    identity::{Mark, PendId, TaskId},
    modifiable::Modifiable,
    pend::{CloneRights, CreationRights, Operations},
    unit::{UnitQuery, UnitTarget},
    wait::Wait,
};
use kernel_client::call::{clone_view, pend, room, unit};
use system_client::{
    Error,
    api::{
        self,
        control::{self, operations::State},
        value::Outcome,
    },
    control::Client,
};
use twine::view::clone::{Receiver, Sender};
use weave::{Codec, transport};

static ROOT: AtomicUsize = AtomicUsize::new(0);
static ENTRY: AtomicUsize = AtomicUsize::new(0);
static REQUESTS: AtomicUsize = AtomicUsize::new(0);
static PAUSED: AtomicBool = AtomicBool::new(false);
static PAUSE: AtomicU64 = AtomicU64::new(0);
static LATE_GO: AtomicBool = AtomicBool::new(false);
static LATE_DONE: AtomicBool = AtomicBool::new(false);
static SPOOF_DONE: AtomicBool = AtomicBool::new(false);
static REJECTS: AtomicUsize = AtomicUsize::new(0);
static INGRESS_QUEUE: AtomicUsize = AtomicUsize::new(0);
static INGRESS_STOP: AtomicBool = AtomicBool::new(false);
static INGRESS_DONE: AtomicBool = AtomicBool::new(false);
static POLL_GO: AtomicBool = AtomicBool::new(false);

core::arch::global_asm!(
    ".section .text.entry",
    ".global _start",
    "_start:",
    "tail start"
);

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let _ = writeln!(Console, "FAIL system-client {info}");
    exit(1)
}

fn request(name: &str) -> control::Named {
    control::Named {
        name: api::Name::new(name).unwrap(),
    }
}

#[unsafe(no_mangle)]
extern "C" fn start() -> ! {
    writeln!(Console, "system-client start").unwrap();
    let license = fixture::license();
    let UnitTarget::Task(root) = fixture::inspect(UnitQuery::Caller).target else {
        unreachable!()
    };
    ROOT.store(root.get(), Ordering::Release);
    let (target, service) = fixture::spawn(server);
    fixture::grant(
        license,
        service,
        Operations::None(CreationRights::CLONE),
        Modifiable::No,
    );
    unit::resume(target).unwrap();
    until(|| ENTRY.load(Ordering::Acquire) != 0);
    let entry = PendId::from_bytes(&(ENTRY.load(Ordering::Acquire) as u64).to_le_bytes()).unwrap();
    assert!(matches!(
        Client::bind(entry, root, license),
        Err(Error::InvalidEndpoint)
    ));
    let mut client = Client::bind(entry, service, license).unwrap();
    let baseline = fixture::active();
    writeln!(Console, "system-client bound").unwrap();

    for name in ["ready", "wrong", "bad", "ready"] {
        writeln!(Console, "system-client call {name}").unwrap();
        let mut pending = client
            .begin::<State>(request(name), Wait::At(deadline()))
            .unwrap();
        let response = pending.wait();
        match name {
            "wrong" => assert!(matches!(response, Err(Error::WrongOperation))),
            "bad" => assert!(matches!(
                response,
                Err(Error::Receive(transport::ReceiveError::Decode))
            )),
            _ => {
                let response = response.unwrap();
                assert_eq!(response.metadata, service);
                assert!(matches!(
                    response.value,
                    Outcome::Success(control::Observation {
                        state: control::State::Ready,
                        ..
                    })
                ));
            }
        }
        assert!(matches!(pending.poll(), Err(Error::Inactive)));
        pending.finish().unwrap();
        pending.finish().unwrap();
        assert!(pending.is_finished());
        assert_eq!(fixture::active(), baseline, "reply/watch/payload leak");
    }

    let mut pending = client
        .begin::<State>(request("spoof"), Wait::At(deadline()))
        .unwrap();
    let reply = fixture::owned_reply(root);
    Sender::new(reply).push(&[255]).unwrap();
    assert!(
        matches!(pending.wait(), Err(Error::Receive(transport::ReceiveError::Transport(
        twine::view::clone::ReplyError::WrongSender { expected, actual }
    ))) if expected == service && actual == root)
    );
    pending.finish().unwrap();
    SPOOF_DONE.store(true, Ordering::Release);
    until(|| REQUESTS.load(Ordering::Acquire) == 5);
    assert_eq!(fixture::active(), baseline);

    let mut pending = client.begin::<State>(request("poll"), Wait::Poll).unwrap();
    assert!(matches!(pending.wait(), Err(Error::NotReady)));
    POLL_GO.store(true, Ordering::Release);
    until(|| pending.poll().unwrap().is_some());
    pending.finish().unwrap();
    assert_eq!(fixture::active(), baseline);

    // Removing SIGN permission forces failure after creating and granting the
    // reply queue. No request may be submitted and the owner must remain usable.
    let limited = fixture::grant(
        license,
        root,
        Operations::None(CreationRights::CLONE),
        Modifiable::No,
    );
    let mut limited_client = Client::bind(entry, service, limited).unwrap();
    let count = REQUESTS.load(Ordering::Acquire);
    let mut failure = match limited_client.begin::<State>(request("ready"), Wait::At(deadline())) {
        Err(failure) => failure,
        Ok(_) => panic!("Sign creation should fail"),
    };
    assert!(matches!(failure.error, Error::CreateWatch(_)));
    assert!(!failure.cleanup.is_finished());
    failure.cleanup.finish().unwrap();
    failure.cleanup.finish().unwrap();
    assert!(failure.cleanup.is_finished());
    assert_eq!(REQUESTS.load(Ordering::Acquire), count);
    pend::return_(limited).unwrap();
    assert_eq!(fixture::active(), baseline);

    let at = room::clock() + 20_000_000;
    let mut pending = client
        .begin::<State>(request("late"), Wait::At(at))
        .unwrap();
    assert!(matches!(pending.wait(), Err(Error::TimedOut)));
    assert!(room::clock() >= at);
    pending.finish().unwrap();
    LATE_GO.store(true, Ordering::Release);
    until(|| LATE_DONE.load(Ordering::Acquire));
    assert_eq!(fixture::active(), baseline);

    // Keep one sentinel queued until after the caller starts. Sign readiness
    // must wake a blocked submit without duplicating its request.
    PAUSE.store(room::clock() + 30_000_000, Ordering::Release);
    until(|| PAUSED.load(Ordering::Acquire));
    Sender::new(entry).push(b"sentinel").unwrap();
    let before = REQUESTS.load(Ordering::Acquire);
    let mut pending = client
        .begin::<State>(request("ready"), Wait::At(deadline()))
        .unwrap();
    pending.wait().unwrap();
    pending.finish().unwrap();
    assert_eq!(REQUESTS.load(Ordering::Acquire), before + 1);
    assert_eq!(fixture::active(), baseline);

    PAUSE.store(deadline(), Ordering::Release);
    until(|| PAUSED.load(Ordering::Acquire));
    Sender::new(entry).push(b"sentinel").unwrap();
    let before = REQUESTS.load(Ordering::Acquire);
    let at = room::clock() + 20_000_000;
    let mut failure = match client.begin::<State>(request("ready"), Wait::At(at)) {
        Err(failure) => failure,
        Ok(_) => panic!("full queue should time out"),
    };
    assert!(matches!(failure.error, Error::TimedOut));
    assert!(room::clock() >= at);
    failure.cleanup.finish().unwrap();
    PAUSE.store(0, Ordering::Release);
    assert_eq!(REQUESTS.load(Ordering::Acquire), before);
    assert_eq!(fixture::active(), baseline);

    pend::return_(entry).unwrap();
    unit::stop(target).unwrap();
    until(|| fixture::inspect(UnitQuery::Target(target)).reclaimed);
    unit::delete(target).unwrap();
    ingress(license, root);
    identity(license, root);
    adjacency::run(license, root);
    actor::run(license, root);
    wait_group::run(license);
    writeln!(Console, "PASS system-client typed=1 sender=1 poll=1 variant=1 decode=1 partial=1 timeout=1 late=1 backpressure=1 send-timeout=1 cleanup=1 ingress=1 route=1 shutdown=1 identity=1").unwrap();
    exit(0)
}

extern "C" fn server() -> ! {
    let license = fixture::license();
    let queue = clone_view::create(
        license,
        Mark::NONE,
        CloneLimits {
            max_len: 1024,
            max_messages: 1,
            max_bytes: 1024,
        },
    )
    .unwrap();
    let root = TaskId::new(ROOT.load(Ordering::Acquire));
    let grant = fixture::grant(
        queue,
        root,
        Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
        Modifiable::Yes,
    );
    ENTRY.store(grant.get(), Ordering::Release);
    let mut receiver = Receiver::new(queue);
    let mut input = [0; 1024];
    let mut output = [0; 1024];
    loop {
        if room::clock() < PAUSE.load(Ordering::Acquire) {
            PAUSED.store(true, Ordering::Release);
            while room::clock() < PAUSE.load(Ordering::Acquire) {
                room::yield_();
            }
            PAUSED.store(false, Ordering::Release);
        }
        let frame = match weave::FrameReceiver::receive(&mut receiver, &mut input) {
            Ok(frame) => frame,
            Err(twine::view::clone::ReceiveError::Pull(error)) if error.source.is_busy() => {
                room::yield_();
                continue;
            }
            Err(twine::view::clone::ReceiveError::Open(error)) if error.source.is_busy() => {
                room::yield_();
                continue;
            }
            Err(error) => panic!("receive {error:?}"),
        };
        if &input[..frame.len] == b"sentinel" {
            continue;
        }
        type Request = api::value::Request<control::operations::Request, 1>;
        let request = Request::decode_exact(&input[..frame.len]).unwrap();
        assert_eq!(frame.metadata, root);
        let control::operations::DecodedRequest::State(requested) = request.body else {
            panic!("wrong operation")
        };
        let name = requested.name.as_str();
        REQUESTS.fetch_add(1, Ordering::AcqRel);
        let route = PendId::try_from(request.reply).unwrap();
        if name == "spoof" {
            until(|| SPOOF_DONE.load(Ordering::Acquire));
            continue;
        }
        let mut sender = Sender::reply(route, frame.metadata).unwrap();
        if name == "poll" {
            until(|| POLL_GO.load(Ordering::Acquire));
        }
        let observation = Outcome::Success(control::Observation {
            state: control::State::Ready,
            reason: 0,
            completed: false,
            task: api::TaskId::new(99),
            team: api::TeamId::new(7),
        });
        let response = if name == "wrong" {
            control::operations::Response::Mint(observation)
        } else {
            control::operations::Response::State(observation)
        };
        if name == "late" {
            until(|| LATE_GO.load(Ordering::Acquire));
            assert!(sender.push(b"late").is_err());
            LATE_DONE.store(true, Ordering::Release);
        } else if name == "bad" {
            sender.push(&[255]).unwrap();
        } else {
            transport::send(&mut sender, &response, &mut output).unwrap();
        }
    }
}

fn ingress(license: PendId, root: TaskId) {
    ENTRY.store(0, Ordering::Release);
    REQUESTS.store(0, Ordering::Release);
    let (target, service) = fixture::spawn(ingress_server);
    fixture::grant(
        license,
        service,
        Operations::None(CreationRights::CLONE | CreationRights::SIGN),
        Modifiable::No,
    );
    unit::resume(target).unwrap();
    until(|| ENTRY.load(Ordering::Acquire) != 0);
    let entry = PendId::from_bytes(&(ENTRY.load(Ordering::Acquire) as u64).to_le_bytes()).unwrap();
    let mut client = Client::bind(entry, service, license).unwrap();
    let baseline = fixture::active();
    let mut pending = client
        .begin::<State>(request("ready"), Wait::At(deadline()))
        .unwrap();
    let received = pending.wait().unwrap();
    assert_eq!(received.metadata, service);
    pending.finish().unwrap();
    assert_eq!(REQUESTS.load(Ordering::Acquire), 1);
    assert_eq!(fixture::active(), baseline);

    // A real serving endpoint must reject a queue it owns when a caller names
    // that queue as a reply grant. No handler is allowed to run first.
    let envelope = api::value::Request::<_, 1> {
        reply: api::PendId::new(INGRESS_QUEUE.load(Ordering::Acquire) as u64),
        body: control::operations::Request::State(request("ready")),
    };
    let mut bytes = [0; 1024];
    let size = weave::codec::encode(&envelope, &mut bytes).unwrap();
    Sender::new(entry).push(&bytes[..size]).unwrap();
    until(|| REJECTS.load(Ordering::Acquire) == 1);
    assert_eq!(REQUESTS.load(Ordering::Acquire), 1);
    bytes[4..6].copy_from_slice(&1_u16.to_le_bytes());
    Sender::new(entry).push(&bytes[..size]).unwrap();
    until(|| REJECTS.load(Ordering::Acquire) == 2);
    assert_eq!(REQUESTS.load(Ordering::Acquire), 1);

    INGRESS_STOP.store(true, Ordering::Release);
    // Wake the service so it can finish its watch and any retained delivery.
    let mut pending = client
        .begin::<State>(request("stop"), Wait::At(deadline()))
        .unwrap();
    pending.wait().unwrap();
    pending.finish().unwrap();
    until(|| INGRESS_DONE.load(Ordering::Acquire));
    until(|| fixture::inspect(UnitQuery::Target(target)).reclaimed);
    unit::delete(target).unwrap();
    assert!(Sender::new(entry).push(b"closed").is_err());
    assert_eq!(root.get(), ROOT.load(Ordering::Acquire));
}

extern "C" fn ingress_server() -> ! {
    let license = fixture::license();
    let queue = clone_view::create(
        license,
        Mark::NONE,
        CloneLimits {
            max_len: 1024,
            max_messages: 1,
            max_bytes: 1024,
        },
    )
    .unwrap();
    let mut endpoint = match protocol::Control::bind(queue, license) {
        Ok(endpoint) => endpoint,
        Err(error) => panic!("bind {error:?}"),
    };
    let root = TaskId::new(ROOT.load(Ordering::Acquire));
    let grant = fixture::grant(
        queue,
        root,
        Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
        Modifiable::Yes,
    );
    INGRESS_QUEUE.store(queue.get(), Ordering::Release);
    ENTRY.store(grant.get(), Ordering::Release);
    loop {
        let result = endpoint.serve_once(Wait::Infinite, |actual, request| {
            assert_eq!(actual, root);
            let control::operations::DecodedRequest::State(_) = request else {
                panic!("wrong operation")
            };
            REQUESTS.fetch_add(1, Ordering::AcqRel);
            control::operations::Response::State(Outcome::Success(control::Observation {
                state: control::State::Ready,
                reason: 0,
                completed: false,
                task: actual.into(),
                team: api::TeamId::new(1),
            }))
        });
        match result {
            Ok(()) if INGRESS_STOP.load(Ordering::Acquire) => break,
            Ok(()) => (),
            Err(protocol::Error::Serve(weave::server::ServeError::Route(_)))
            | Err(protocol::Error::Serve(weave::server::ServeError::Receive(
                transport::ReceiveError::Decode,
            ))) => {
                REJECTS.fetch_add(1, Ordering::AcqRel);
            }
            Err(error) => panic!("serve {error:?}"),
        }
    }
    endpoint.finish().unwrap();
    endpoint.finish().unwrap();
    assert!(endpoint.is_finished());
    assert!(matches!(
        endpoint.serve_once(Wait::Poll, |_, _| unreachable!()),
        Err(protocol::Error::Inactive)
    ));
    pend::return_(queue).unwrap();
    INGRESS_DONE.store(true, Ordering::Release);
    exit(0)
}

fn identity(license: PendId, root: TaskId) {
    use api::identity::{self as id, operations as op};
    ENTRY.store(0, Ordering::Release);
    let (target, service) = fixture::spawn(identity_server);
    fixture::grant(
        license,
        service,
        Operations::None(CreationRights::CLONE | CreationRights::SIGN),
        Modifiable::No,
    );
    unit::resume(target).unwrap();
    until(|| ENTRY.load(Ordering::Acquire) != 0);
    let entry = PendId::from_bytes(&(ENTRY.load(Ordering::Acquire) as u64).to_le_bytes()).unwrap();
    let mut client = system_client::identity::Client::bind(entry, service, license).unwrap();
    let subject = id::Subject::new(
        id::PrincipalId {
            authority: service.into(),
            slot: 0,
        },
        &[],
    )
    .unwrap();
    let id = id::InstallId::new(1).unwrap();
    macro_rules! call {
        ($op:ty, $body:expr) => {{
            let mut pending = client.begin::<$op>($body, Wait::At(deadline())).unwrap();
            let response = pending.wait().unwrap();
            assert_eq!(response.metadata, service);
            pending.finish().unwrap();
            response.value
        }};
    }
    let apply = |id| id::InstallRequest {
        id,
        task: root.into(),
        install: id::Install::Authorized(subject.clone()),
    };
    assert!(matches!(
        call!(op::Install, apply(id)),
        Outcome::Success(id::InstallStatus {
            state: weave::Coded(id::InstallState::Installed),
            ..
        })
    ));
    assert!(matches!(
        call!(op::Inspect, id),
        Outcome::Success(id::InstallStatus {
            state: weave::Coded(id::InstallState::Installed),
            ..
        })
    ));
    assert!(matches!(
        call!(op::Cancel, id),
        Outcome::Success(id::InstallStatus {
            state: weave::Coded(id::InstallState::Cancelled),
            ..
        })
    ));
    assert!(matches!(
        call!(op::Inspect, id),
        Outcome::Success(id::InstallStatus {
            state: weave::Coded(id::InstallState::Cancelled),
            ..
        })
    ));
    assert!(matches!(
        call!(op::Install, apply(id)),
        Outcome::Success(id::InstallStatus {
            state: weave::Coded(id::InstallState::Cancelled),
            binding: api::value::Optional(None),
            ..
        })
    ));
    let second = id::InstallId::new(2).unwrap();
    assert!(matches!(
        call!(op::Install, apply(second)),
        Outcome::Success(id::InstallStatus {
            state: weave::Coded(id::InstallState::Installed),
            ..
        })
    ));
    assert!(matches!(
        call!(op::Retire, second),
        Outcome::Success(id::InstallStatus {
            state: weave::Coded(id::InstallState::Retired),
            binding: api::value::Optional(None),
            ..
        })
    ));
    assert!(matches!(
        call!(
            op::Same,
            id::Same {
                left: root.into(),
                right: root.into()
            }
        ),
        Outcome::Failure(id::Fail::Denied)
    ));
    until(|| fixture::inspect(UnitQuery::Target(target)).reclaimed);
    assert_eq!(fixture::inspect(UnitQuery::Target(target)).result, Some(0));
    unit::delete(target).unwrap();
}

extern "C" fn identity_server() -> ! {
    let license = fixture::license();
    let UnitTarget::Task(task) = fixture::inspect(UnitQuery::Caller).target else {
        unreachable!()
    };
    let root = TaskId::new(ROOT.load(Ordering::Acquire));
    let mut book = identity_book::IdentityBook::new(task.into(), root.into()).unwrap();
    let queue = clone_view::create(
        license,
        Mark::NONE,
        CloneLimits {
            max_len: 4096,
            max_messages: 1,
            max_bytes: 4096,
        },
    )
    .unwrap();
    let mut endpoint = match protocol::Identity::bind(queue, license) {
        Ok(endpoint) => endpoint,
        Err(error) => panic!("identity bind {error:?}"),
    };
    let grant = fixture::grant(
        queue,
        root,
        Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
        Modifiable::Yes,
    );
    ENTRY.store(grant.get(), Ordering::Release);
    for _ in 0..8 {
        endpoint
            .serve_once(Wait::At(deadline()), |caller, request| {
                identity_answer::answer(
                    &mut book,
                    identity_answer::Context {
                        from: caller.into(),
                        endpoint: queue.into(),
                        grant: api::identity::Grant::Install,
                    },
                    request,
                )
            })
            .unwrap();
    }
    endpoint.finish().unwrap();
    pend::return_(queue).unwrap();
    exit(0)
}

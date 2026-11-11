use crate::{
    fixture::{self, Console, addr, deadline, until},
    identity_service::{Config, Epoch, Error, Service},
    revision,
};
use alloc::boxed::Box;
use core::{
    fmt::Write as _,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use kernel_api::{
    abi::call::sign::SignReply,
    address::VirtAddr,
    model::{
        identity::{PendId, TaskId},
        modifiable::Modifiable,
        pend::{
            AccordRequest, AccordTarget, CreationRights, Distributable, Limits, Operations,
            PendRange,
        },
        sign::SignRights,
        unit::{UnitCreate, UnitQuery, UnitTarget},
        wait::Wait,
    },
};
use kernel_client::call::{pend, room, sign, unit};
use system_client::{
    api::{
        self,
        identity::{self as id, operations as op},
        value::Outcome,
    },
    identity::Client,
};
use twine::view::clone::{Reply, Sender};
use weave::transport;

static GRANTS: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
static PUBLICATIONS: [AtomicUsize; 20] = [const { AtomicUsize::new(0) }; 20];
static READY: AtomicBool = AtomicBool::new(false);
static GO: AtomicBool = AtomicBool::new(false);
static DONE: AtomicBool = AtomicBool::new(false);
static HANDLED: AtomicUsize = AtomicUsize::new(0);
static REJECTED: AtomicUsize = AtomicUsize::new(0);
static MAX_REJECTED: AtomicUsize = AtomicUsize::new(0);
static REPLY_FAILURES: AtomicUsize = AtomicUsize::new(0);
static OPERATOR_ENTRY: AtomicUsize = AtomicUsize::new(0);

fn native(raw: usize) -> PendId {
    PendId::from_bytes(&(raw as u64).to_le_bytes()).unwrap()
}
fn endpoint(grant: id::Grant) -> PendId {
    native(PUBLICATIONS[grant as usize - 1].load(Ordering::Acquire))
}

pub fn run(license: PendId, root: TaskId) {
    for mode in 0..3 {
        scenario(license, root, mode);
    }
    writeln!(Console, "PASS identity-actor faces=20 fair=1 revision=1 coalesced=1 failed-reply=1 revoked=1 partial=1 shutdown=1").unwrap();
}

fn scenario(license: PendId, root: TaskId, mode: usize) {
    READY.store(false, Ordering::Release);
    GO.store(false, Ordering::Release);
    DONE.store(false, Ordering::Release);
    HANDLED.store(0, Ordering::Release);
    REJECTED.store(0, Ordering::Release);
    MAX_REJECTED.store(0, Ordering::Release);
    REPLY_FAILURES.store(0, Ordering::Release);
    let baseline = fixture::active();
    let shutdown = sign::create(license).unwrap();
    let notify = sign::create(license).unwrap();
    let (dependency, operator, operator_target) = if mode == 1 {
        OPERATOR_ENTRY.store(0, Ordering::Release);
        let (target, task) = fixture::spawn(operator_worker);
        fixture::grant(
            license,
            task,
            Operations::None(CreationRights::SIGN),
            Modifiable::No,
        );
        unit::resume(target).unwrap();
        until(|| OPERATOR_ENTRY.load(Ordering::Acquire) != 0);
        (
            native(OPERATOR_ENTRY.load(Ordering::Acquire)),
            task,
            Some(target),
        )
    } else {
        (sign::create(license).unwrap(), root, None)
    };
    let epoch = Epoch::new();
    let epoch_pointer = Box::into_raw(Box::new(epoch.clone()));
    let arguments = [epoch_pointer.addr(), mode, operator.get()];
    let create = UnitCreate::Task {
        team: None,
        entry: VirtAddr::new(worker as *const () as usize),
        args: VirtAddr::new(arguments.as_ptr().addr()),
        count: 3,
        stack: VirtAddr::new(128 * 1024),
    };
    let target = unit::create(addr(&create.to_bytes())).unwrap();
    let UnitTarget::Task(task) = target else {
        unreachable!()
    };
    let rights = if mode == 2 {
        CreationRights::CLONE
    } else {
        CreationRights::CLONE | CreationRights::SIGN
    };
    fixture::grant(license, task, Operations::None(rights), Modifiable::No);
    for (index, (source, rights)) in [
        (
            shutdown,
            SignRights::LISTEN | SignRights::CONSUME | SignRights::ROUTE,
        ),
        (notify, SignRights::TRIGGER),
        (dependency, SignRights::LISTEN),
    ]
    .into_iter()
    .enumerate()
    {
        let granted = fixture::grant(source, task, Operations::Sign(rights), Modifiable::No);
        GRANTS[index].store(granted.get(), Ordering::Release);
    }
    unit::resume(target).unwrap();
    if mode == 2 {
        until(|| DONE.load(Ordering::Acquire));
    } else {
        until(|| READY.load(Ordering::Acquire));
        for grant in id::Grant::ALL {
            assert!(PUBLICATIONS[grant as usize - 1].load(Ordering::Acquire) != 0);
        }
        if mode == 1 {
            GO.store(true, Ordering::Release);
            let target = operator_target.unwrap();
            unit::stop(target).unwrap();
            until(|| fixture::inspect(UnitQuery::Target(target)).reclaimed);
            unit::delete(target).unwrap();
            until(|| DONE.load(Ordering::Acquire));
        } else {
            let subject = id::Subject::new(
                id::PrincipalId {
                    authority: task.into(),
                    slot: 0,
                },
                &[],
            )
            .unwrap();
            let binding = || id::Bind {
                task: root.into(),
                install: id::Install::Authorized(subject.clone()),
            };
            for _ in 0..16 {
                Sender::new(endpoint(id::Grant::Resolve))
                    .push(&[255])
                    .unwrap();
            }
            let mut binder = Client::bind(endpoint(id::Grant::Bind), task, license).unwrap();
            let mut first = binder
                .begin::<op::Bind>(binding(), Wait::At(deadline()))
                .unwrap();
            GO.store(true, Ordering::Release);
            assert!(matches!(first.wait().unwrap().value, Outcome::Success(_)));
            first.finish().unwrap();
            until(|| HANDLED.load(Ordering::Acquire) >= 1);
            assert!(MAX_REJECTED.load(Ordering::Acquire) <= 1);
            assert_eq!(revision::consume(notify, &epoch).unwrap(), 1);
            assert_eq!(
                sign::listen(notify, Wait::Poll).unwrap(),
                SignReply::Pending
            );
            macro_rules! call {
                ($grant:ident, $op:ty, $body:expr) => {{
                    let mut client =
                        Client::bind(endpoint(id::Grant::$grant), task, license).unwrap();
                    let mut pending = client.begin::<$op>($body, Wait::At(deadline())).unwrap();
                    let response = pending.wait().unwrap();
                    pending.finish().unwrap();
                    response.value
                }};
            }
            assert!(matches!(
                call!(Resolve, op::Resolve, id::Task { task: root.into() }),
                Outcome::Success(_)
            ));
            until(|| REJECTED.load(Ordering::Acquire) == 16);
            assert_eq!(epoch.current(), 1);
            assert!(matches!(
                call!(Bind, op::Resolve, id::Task { task: root.into() }),
                Outcome::Failure(id::Fail::Denied)
            ));
            assert_eq!(epoch.current(), 1);
            call!(
                Install,
                op::Install,
                id::InstallRequest {
                    id: id::InstallId::new(1).unwrap(),
                    task: root.into(),
                    install: id::Install::Authorized(subject.clone())
                }
            );
            call!(Unbind, op::Unbind, id::Task { task: root.into() });
            assert_eq!(revision::consume(notify, &epoch).unwrap(), 3);
            assert_eq!(
                sign::listen(notify, Wait::Poll).unwrap(),
                SignReply::Pending
            );
            let mut reply = Reply::create(license, 4096).unwrap();
            let route = reply.grant(task).unwrap();
            let kernel_api::model::sign::SignSource::Clone { token, .. } = reply.source() else {
                unreachable!()
            };
            Sender::new(token).push(&[255]).unwrap();
            let request = api::value::Request::<_, 2> {
                reply: route.into(),
                body: op::Request::Bind(binding()),
            };
            let mut bytes = [0; 4096];
            transport::send(
                &mut Sender::new(endpoint(id::Grant::Bind)),
                &request,
                &mut bytes,
            )
            .unwrap();
            until(|| REPLY_FAILURES.load(Ordering::Acquire) == 1);
            reply.finish().unwrap();
            assert_eq!(revision::consume(notify, &epoch).unwrap(), 4);
            assert!(matches!(
                call!(Resolve, op::Resolve, id::Task { task: root.into() }),
                Outcome::Success(_)
            ));
            assert_eq!(epoch.current(), 4);
            // Repeated real reply grants compete with the actor's receive and
            // cleanup paths on SMP. Read-only calls must not advance revision.
            for _ in 0..32 {
                assert!(matches!(
                    call!(Resolve, op::Resolve, id::Task { task: root.into() }),
                    Outcome::Success(_)
                ));
            }
            assert_eq!(epoch.current(), 4);
            sign::trigger(shutdown).unwrap();
            until(|| DONE.load(Ordering::Acquire));
        }
        assert!(
            Sender::new(endpoint(id::Grant::Resolve))
                .push(b"closed")
                .is_err()
        );
    }
    until(|| fixture::inspect(UnitQuery::Target(target)).reclaimed);
    assert_eq!(
        fixture::inspect(UnitQuery::Target(target)).result,
        Some(mode as u64)
    );
    unit::delete(target).unwrap();
    pend::return_(shutdown).unwrap();
    pend::return_(notify).unwrap();
    if mode != 1 {
        pend::return_(dependency).unwrap();
    }
    assert_eq!(
        fixture::active(),
        baseline,
        "actor leaked a granted endpoint or control signal"
    );
}

extern "C" fn worker(arguments: usize, count: usize) -> ! {
    assert_eq!(count, 3);
    // SAFETY: Kernel copied these words to this Task's stack. The Epoch box is
    // transferred exactly once within the same Team; it contains no Pend.
    let args = unsafe { core::slice::from_raw_parts(arguments as *const usize, count) };
    let epoch = unsafe { *Box::from_raw(args[0] as *mut Epoch) };
    let mode = args[1];
    let root = TaskId::new(crate::ROOT.load(Ordering::Acquire));
    let config = Config {
        license: fixture::license(),
        controller: root,
        operator: TaskId::new(args[2]),
        shutdown: native(GRANTS[0].load(Ordering::Acquire)),
        notify: native(GRANTS[1].load(Ordering::Acquire)),
        dependency: native(GRANTS[2].load(Ordering::Acquire)),
        epoch,
    };
    let mut service = match Service::create(config) {
        Ok(service) => {
            assert_ne!(mode, 2);
            service
        }
        Err(mut failure) => {
            assert_eq!(mode, 2);
            assert!(matches!(failure.error, Error::Sign(_)));
            failure.service.finish().unwrap();
            failure.service.finish().unwrap();
            assert!(failure.service.is_finished());
            DONE.store(true, Ordering::Release);
            fixture::exit(mode as u64)
        }
    };
    for publication in service.publications() {
        PUBLICATIONS[publication.grant as usize - 1]
            .store(publication.token.get() as usize, Ordering::Release);
    }
    READY.store(true, Ordering::Release);
    until(|| GO.load(Ordering::Acquire));
    loop {
        match service.step(Wait::Infinite) {
            Ok(report) => {
                HANDLED.fetch_add(report.handled, Ordering::AcqRel);
                REJECTED.fetch_add(report.rejected, Ordering::AcqRel);
                MAX_REJECTED.fetch_max(report.rejected, Ordering::AcqRel);
                REPLY_FAILURES.fetch_add(report.reply_failures, Ordering::AcqRel);
                if report.stopped {
                    assert_eq!(mode, 0);
                    break;
                }
            }
            Err(Error::Inspect(_) | Error::Dependency) if mode == 1 => break,
            Err(error) => panic!("actor progress {error:?}"),
        }
        room::yield_();
    }
    service.finish().unwrap();
    service.finish().unwrap();
    assert!(service.is_finished());
    assert!(matches!(service.step(Wait::Poll), Err(Error::Inactive)));
    DONE.store(true, Ordering::Release);
    fixture::exit(mode as u64)
}

extern "C" fn operator_worker() -> ! {
    let token = sign::create(fixture::license()).unwrap();
    let root = TaskId::new(crate::ROOT.load(Ordering::Acquire));
    let request = AccordRequest {
        src: token,
        target: AccordTarget::Task(root),
        mark: kernel_api::model::identity::Mark::NONE,
        limits: Limits {
            operations: Operations::Sign(SignRights::LISTEN),
            modifiable: Modifiable::No,
            distributable: Distributable::Yes,
            range: PendRange::Inherit,
        },
    };
    let granted = pend::accord(addr(&request.to_bytes())).unwrap();
    OPERATOR_ENTRY.store(granted.get(), Ordering::Release);
    loop {
        room::sleep_until(u64::MAX).unwrap();
    }
}

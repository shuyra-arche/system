use crate::{
    fixture::{self, Console},
    native_loader::{Image, Job, Loader, ProgramKind, Spawn},
};
use core::fmt::Write as _;
use kernel_api::model::{
    identity::PendId,
    unit::{ControlId, ExitCause, UnitState},
};
static IMAGE: &[u8] = include_bytes!(env!("SYSTEM_LOADER_IMAGE"));
fn prepare(loader: &mut Loader, args: &[usize]) -> Job {
    loader
        .prepare(
            Image {
                bytes: IMAGE,
                kind: ProgramKind::User,
            },
            Spawn {
                args,
                stack: 128 * 1024,
                guard: ControlId::NONE,
                member_exit: true,
                creation: kernel_api::model::pend::CreationRights::FLAT
                    | kernel_api::model::pend::CreationRights::SIGN,
            },
        )
        .unwrap()
}
fn ready(job: &mut Job, loader: &mut Loader) {
    fixture::until(|| {
        loader.advance().unwrap();
        job.advance(loader).unwrap()
    });
    assert_eq!(job.phase(), crate::native_loader::Phase::Dormant);
    assert!(job.team().is_some() && job.task().is_some());
    assert_eq!(job.observe().unwrap().unwrap().state, UnitState::Dormant);
    assert!(!job.finish().unwrap());
}
fn finish(job: &mut Job) {
    fixture::until(|| match job.finish() {
        Ok(done) => done,
        Err(error) if error.busy() => false,
        Err(error) => panic!("ELF retirement {error:?}"),
    });
    assert!(job.finish().unwrap());
}
pub fn run(license: PendId) {
    // SAFETY: the fixture takes Space from its trusted initial-Task ledger.
    let mut loader = unsafe { Loader::new(license, fixture::space()) }.unwrap();
    let baseline = fixture::active();
    assert!(
        loader
            .prepare(
                Image {
                    bytes: IMAGE,
                    kind: ProgramKind::Supervisor
                },
                Spawn {
                    args: &[],
                    stack: 128 * 1024,
                    guard: ControlId::NONE,
                    member_exit: true,
                    creation: kernel_api::model::pend::CreationRights::FLAT
                        | kernel_api::model::pend::CreationRights::SIGN,
                }
            )
            .is_err()
    );
    for rounds in [0, 1, 4, 8, 16, 32, 64, 128] {
        let mut partial = prepare(&mut loader, &[0xa5, 0x5a]);
        for _ in 0..rounds {
            if partial.advance(&mut loader).unwrap() {
                break;
            }
        }
        partial.cancel().unwrap();
        finish(&mut partial);
        drop(partial);
        fixture::until(|| loader.clear_images().unwrap());
        assert_eq!(
            fixture::active(),
            baseline,
            "partial ELF construction left resources at round {rounds}"
        );
    }
    let mut first = prepare(&mut loader, &[0xa5, 0x5a]);
    ready(&mut first, &mut loader);
    let entries = loader.cached_entries();
    assert!(entries > 0);
    let mut second = prepare(&mut loader, &[0xa5, 0x5a]);
    ready(&mut second, &mut loader);
    assert_eq!(
        loader.cached_entries(),
        entries,
        "identical RX bytes must reuse the immutable cache"
    );
    first.resume().unwrap();
    finish(&mut first);
    let receipt = first.result().unwrap();
    assert_eq!(receipt.exit, Some((ExitCause::Reap, 0)));
    assert_eq!(receipt.result, Some(17));
    assert!(receipt.reclaimed);
    fixture::until(|| loader.clear_images().unwrap());
    assert_eq!(loader.cached_entries(), 0);
    second.resume().unwrap();
    finish(&mut second);
    let receipt = second.result().unwrap();
    assert_eq!(
        receipt.exit,
        Some((ExitCause::Reap, 0)),
        "DATA/BSS must be private and RX must survive cache eviction"
    );
    assert_eq!(receipt.result, Some(17));
    assert!(receipt.reclaimed);
    drop(first);
    drop(second);
    let mut running = prepare(&mut loader, &[0xa5, 0x5b]);
    ready(&mut running, &mut loader);
    let signal = kernel_client::call::sign::create(license).unwrap();
    let request = kernel_api::model::pend::AccordRequest {
        src: signal,
        target: kernel_api::model::pend::AccordTarget::Task(running.task().unwrap()),
        mark: kernel_api::model::identity::Mark::of("system-loader/hold"),
        limits: kernel_api::model::pend::Limits {
            operations: kernel_api::model::pend::Operations::Sign(
                kernel_api::model::sign::SignRights::TRIGGER,
            ),
            modifiable: kernel_api::model::modifiable::Modifiable::No,
            distributable: kernel_api::model::pend::Distributable::No,
            scope: kernel_api::model::pend::PendScope::Object,
        },
    }
    .to_bytes();
    kernel_client::call::pend::accord(kernel_api::address::VirtAddr::new(request.as_ptr().addr()))
        .unwrap();
    running.resume().unwrap();
    // A real Trigger from the image proves it entered and finished its first
    // native allocation, before the parent requests Stop rather than waiting.
    fixture::until(|| {
        match kernel_client::call::sign::listen(signal, kernel_api::model::wait::Wait::Poll)
            .unwrap()
        {
            kernel_api::abi::call::sign::SignReply::Pending => false,
            kernel_api::abi::call::sign::SignReply::Triggered { sequence } => {
                kernel_client::call::sign::consume(signal, sequence).unwrap();
                true
            }
            reply => panic!("unexpected image signal: {reply:?}"),
        }
    });
    running.cancel().unwrap();
    finish(&mut running);
    let receipt = running.result().unwrap();
    assert_eq!(receipt.state, UnitState::Exited);
    assert!(receipt.reclaimed);
    drop(running);
    kernel_client::call::pend::return_(signal).unwrap();
    fixture::until(|| loader.clear_images().unwrap());
    let mut empty = prepare(&mut loader, &[]);
    ready(&mut empty, &mut loader);
    empty.resume().unwrap();
    finish(&mut empty);
    assert_eq!(empty.result().unwrap().result, Some(17));
    assert!(empty.result().unwrap().reclaimed);
    drop(empty);
    fixture::until(|| loader.clear_images().unwrap());
    assert_eq!(
        fixture::active(),
        baseline,
        "native ELF/Team retirement leaked caller grants"
    );
    writeln!(Console, "PASS loader-elf native=1 user=1 arguments=1 private=1 cache=1 eviction=1 cancel=1 running-stop=1 journal=1 reclaim=1").unwrap();
}

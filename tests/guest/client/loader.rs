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
fn prepare(loader: &mut Loader) -> Job {
    loader
        .prepare(
            Image {
                bytes: IMAGE,
                kind: ProgramKind::User,
            },
            Spawn {
                args: &[0xa5, 0x5a],
                stack: 128 * 1024,
                guard: ControlId::NONE,
                member_exit: true,
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
fn initialized(job: &mut Job) {
    fixture::until(|| {
        match job.initialized() {
            Ok(true) => return true,
            Ok(false) => (),
            Err(error) if error.busy() => (),
            Err(error) => panic!("initializer {error:?}"),
        }
        let info = job.observe().unwrap().unwrap();
        assert_ne!(
            info.state,
            UnitState::Exited,
            "initializer exited before ready: {info:?}"
        );
        false
    });
}
pub fn run(license: PendId) {
    let baseline = fixture::active();
    let mut loader = Loader::new(license).unwrap();
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
                    member_exit: true
                }
            )
            .is_err()
    );
    for rounds in [0, 1, 4, 8] {
        let mut partial = prepare(&mut loader);
        for _ in 0..rounds {
            assert!(!partial.advance(&mut loader).unwrap());
        }
        partial.cancel().unwrap();
        finish(&mut partial);
        fixture::until(|| loader.clear_images().unwrap());
        assert_eq!(
            fixture::active(),
            baseline,
            "partial ELF construction left resources at round {rounds}"
        );
    }
    let mut first = prepare(&mut loader);
    ready(&mut first, &mut loader);
    let entries = loader.cached_entries();
    assert!(entries > 0);
    let mut second = prepare(&mut loader);
    ready(&mut second, &mut loader);
    assert_eq!(
        loader.cached_entries(),
        entries,
        "identical RX bytes must reuse the immutable cache"
    );
    first.resume().unwrap();
    initialized(&mut first);
    finish(&mut first);
    let receipt = first.result().unwrap();
    assert_eq!(receipt.exit, Some((ExitCause::Reap, 0)));
    assert_eq!(receipt.result, Some(17));
    assert!(receipt.reclaimed);
    fixture::until(|| loader.clear_images().unwrap());
    assert_eq!(loader.cached_entries(), 0);
    second.resume().unwrap();
    initialized(&mut second);
    finish(&mut second);
    let receipt = second.result().unwrap();
    assert_eq!(
        receipt.exit,
        Some((ExitCause::Reap, 0)),
        "DATA/BSS must be private and RX must survive cache eviction"
    );
    assert_eq!(receipt.result, Some(17));
    assert!(receipt.reclaimed);
    assert_eq!(
        fixture::active(),
        baseline,
        "native ELF/Team retirement leaked caller grants"
    );
    writeln!(Console, "PASS loader-elf native=1 user=1 arguments=1 private=1 cache=1 eviction=1 cancel=1 reclaim=1").unwrap();
}

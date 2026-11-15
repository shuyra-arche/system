//! The production Loader starts a receipt-checked Toolchain std ELF.
use crate::{
    fixture,
    native_loader::{Image, Loader, Phase, ProgramKind, Spawn},
};
use core::fmt::Write;
use kernel_api::model::{
    pend::CreationRights,
    unit::{ControlId, ExitCause, UnitInfo, UnitQuery, UnitState, UnitTarget},
};
static IMAGE: &[u8] = include_bytes!(env!("SYSTEM_TOOLCHAIN_IMAGE"));
pub fn run() {
    let baseline = fixture::active();
    // SAFETY: these are the current Task's genuine initial Ledger grants.
    let mut loader = unsafe { Loader::new(fixture::license(), fixture::space()) }.unwrap();
    let mut job = loader
        .prepare(
            Image {
                bytes: IMAGE,
                kind: ProgramKind::User,
            },
            Spawn {
                args: &[],
                stack: 128 * 1024,
                guard: ControlId::NONE,
                member_exit: true,
                creation: CreationRights::FLAT | CreationRights::SIGN,
            },
        )
        .unwrap();
    fixture::until(|| match job.advance(&mut loader) {
        Ok(done) => done,
        Err(error) if error.busy() => false,
        Err(error) => panic!("std construction: {error:?}"),
    });
    assert_eq!(job.phase(), Phase::Dormant);
    job.resume().unwrap();
    let team = job.team().unwrap();
    let mut process = None;
    fixture::until(|| {
        let mut bytes = [0; UnitInfo::WIDTH];
        assert!(
            kernel_client::call::unit::inspect(
                UnitQuery::Target(UnitTarget::Team(team)),
                kernel_api::address::VirtAddr::new(bytes.as_mut_ptr() as usize)
            )
            .unwrap()
        );
        let info = UnitInfo::from_bytes(&bytes).unwrap();
        let exited = info.state == UnitState::Exited;
        process = Some(info);
        crate::native_heap::poll();
        exited
    });
    let process = process.unwrap();
    assert_eq!(process.result, Some(0));
    assert_eq!(process.exit, Some((ExitCause::Reap, 0)));
    fixture::until(|| match job.finish() {
        Ok(done) => done,
        Err(error) if error.busy() => false,
        Err(error) => panic!("std retirement: {error:?}"),
    });
    let info = job.result().unwrap();
    assert!(info.reclaimed);
    // Rust main exits the whole Team. The broker's own Task receipt need not
    // claim the main Task's result; the process receipt above owns that fact.
    drop(job);
    fixture::until(|| loader.clear_images().unwrap());
    drop(loader);
    assert_eq!(
        fixture::active(),
        baseline,
        "std Loader resources remained live"
    );
    let _ = writeln!(
        fixture::Console,
        "PASS loader-toolchain user=1 std=1 threads=1 reclaim=1 retirement=1"
    );
}

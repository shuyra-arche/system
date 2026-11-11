//! Exercise production supervisor worker ownership on the real Kernel.
use crate::{
    fixture::{self, Console},
    worker::{Phase, Worker},
};
use core::{
    fmt::Write as _,
    sync::atomic::{AtomicUsize, Ordering},
};
use kernel_api::{
    address::VirtAddr,
    model::unit::{ExitCause, ExitScope, UnitState},
};
use kernel_client::call::{room, unit};
static ENTERED: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn worker(words: *const usize, len: usize) -> ! {
    assert_eq!(len, 2);
    // SAFETY: Kernel copied exactly these two words to this Task's live stack.
    let words = unsafe { core::slice::from_raw_parts(words, len) };
    ENTERED.store(words[0], Ordering::Release);
    if words[1] == 1 {
        loop {
            room::yield_();
        }
    }
    unit::exit(17, 0, VirtAddr::new(0), 0, ExitScope::Task)
}
fn settle(worker: &mut Worker) {
    fixture::until(|| match worker.finish() {
        Ok(done) => done,
        Err(crate::worker::Error::Kernel(error)) if error.source.is_busy() => false,
        Err(error) => panic!("worker cleanup {error:?}"),
    });
    assert_eq!(worker.phase(), Phase::Closed);
    assert!(worker.finish().unwrap());
}
pub fn run() {
    ENTERED.store(0, Ordering::Release);
    let mut words = [0xabc, 0];
    // SAFETY: entry uses only the two copied integers, no cross-Task borrows.
    let mut ordinary = unsafe { Worker::prepare(worker, &words, 128 * 1024) }.unwrap();
    assert_ne!(ordinary.task().get(), 0);
    assert_eq!(ordinary.phase(), Phase::Prepared);
    assert_eq!(ordinary.observe().unwrap().state, UnitState::Dormant);
    assert!(!ordinary.finish().unwrap());
    assert_eq!(ENTERED.load(Ordering::Acquire), 0);
    words[0] = 0xdef;
    core::hint::black_box(&words);
    ordinary.resume().unwrap();
    assert!(ordinary.resume().is_err());
    settle(&mut ordinary);
    assert_eq!(
        ENTERED.load(Ordering::Acquire),
        0xabc,
        "Create must snapshot the arguments"
    );
    let result = ordinary.result().unwrap();
    assert_eq!(result.exit, Some((ExitCause::Reap, 0)));
    assert_eq!(result.result, Some(17));
    assert!(result.reclaimed);

    ENTERED.store(0, Ordering::Release);
    // SAFETY: same fixed, integer-only entry contract.
    let mut dormant = unsafe { Worker::prepare(worker, &[9, 0], 128 * 1024) }.unwrap();
    dormant.stop().unwrap();
    settle(&mut dormant);
    assert_eq!(ENTERED.load(Ordering::Acquire), 0);
    assert_eq!(dormant.result().unwrap().exit.unwrap().0, ExitCause::Slay);

    // SAFETY: same fixed, integer-only entry contract.
    let mut running = unsafe { Worker::prepare(worker, &[12, 1], 128 * 1024) }.unwrap();
    running.resume().unwrap();
    fixture::until(|| ENTERED.load(Ordering::Acquire) == 12);
    assert!(!running.finish().unwrap());
    running.stop().unwrap();
    settle(&mut running);
    assert_eq!(running.result().unwrap().exit.unwrap().0, ExitCause::Slay);
    writeln!(
        Console,
        "PASS supervisor-worker native=1 dormant=1 arguments=1 exit=1 cancel=1 reclaim=1"
    )
    .unwrap();
}

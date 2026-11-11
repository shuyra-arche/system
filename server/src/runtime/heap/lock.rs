//! ROOT satisfies a worker's backing request while waiting for its heap lock.
use core::sync::atomic::{
    AtomicUsize,
    Ordering::{Acquire, Relaxed, Release},
};
use kernel_api::model::{
    identity::TaskId,
    unit::{UnitQuery, UnitState},
};
use talc::lock_api::{GuardNoSend, RawMutex};
pub(super) struct Lock(AtomicUsize);
// SAFETY: a unique Task ID is held until release. Acquire/release protects Talc
// metadata; ROOT's service path never enters Talc or touches that metadata.
unsafe impl RawMutex for Lock {
    const INIT: Self = Self(AtomicUsize::new(0));
    type GuardMarker = GuardNoSend;
    fn lock(&self) {
        let caller = super::caller().unwrap_or_else(|| super::fatal()).get();
        while self
            .0
            .compare_exchange(0, caller, Acquire, Relaxed)
            .is_err()
        {
            if caller == super::ROOT.load(Acquire) {
                super::service();
                let owner = self.0.load(Acquire);
                if owner != 0 {
                    let dead = super::inspect(UnitQuery::Target(TaskId::new(owner).into()))
                        .is_none_or(|info| info.state == UnitState::Exited);
                    // It may have unlocked and exited during inspection.
                    if dead && self.0.load(Acquire) == owner {
                        super::fatal();
                    }
                }
            }
            kernel_client::call::room::yield_();
        }
    }
    fn try_lock(&self) -> bool {
        let caller = super::caller().unwrap_or_else(|| super::fatal()).get();
        self.0.compare_exchange(0, caller, Acquire, Relaxed).is_ok()
    }
    unsafe fn unlock(&self) {
        self.0.store(0, Release);
    }
}

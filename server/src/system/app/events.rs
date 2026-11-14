//! Product lifecycle observations derived from the supervisor's real receipts.
use unit_contract::event::{self, Event};
pub(crate) fn emit(event: Event<&str>) {
    if let Ok(frame) = event::encode(&event) {
        crate::debug::put(&frame);
    }
}

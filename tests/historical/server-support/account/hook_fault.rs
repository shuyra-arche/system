use super::signals::Signals;
use alloc::sync::Arc;
use env::TaskId;

#[derive(Default)]
pub(super) struct HookFault {
    pub(super) prepare: bool,
    pub(super) failed_task: Option<TaskId>,
    pub(super) waited: bool,
    pub(super) retired_failed: bool,
    pub(super) waiting: Option<usize>,
    pub(super) signals: Option<Arc<Signals>>,
}

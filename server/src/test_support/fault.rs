use env::TaskId;

pub(super) struct FaultResource {
    pub(super) armed: bool,
    pub(super) task: Option<TaskId>,
    pub(super) prepared: bool,
}

use super::state::FixtureState;
use env::TaskId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstanceView {
    pub task: TaskId,
    pub state: FixtureState,
    pub team: Option<env::TeamId>,
    pub prepared: bool,
}

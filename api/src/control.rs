//! Supervision intent, lifecycle observations and restart requests.
use crate::{
    Mark, Name, TaskId, TeamId,
    value::{Optional, Outcome},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum State {
    NeverStarted = 0,
    Starting = 1,
    Ready = 2,
    Stopping = 3,
    Dead = 4,
    Debarked = 5,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Fail {
    Unknown = 1,
    BadImage = 2,
    Full = 3,
    NotReady = 4,
    Bad = 5,
    Denied = 6,
    Conflict = 7,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Grant {
    State = 1,
    Mint = 2,
    Embark = 3,
    Debark = 4,
    Ruin = 5,
    Observe = 6,
    Recover = 7,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Named {
    pub name: Name,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Instance {
    pub task: TaskId,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Publisher {
    pub namespace: Mark,
    pub resource: Mark,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Observation {
    #[frame(code)]
    pub state: State,
    pub reason: u64,
    pub completed: bool,
    pub task: TaskId,
    pub team: TeamId,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Position {
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Row {
    pub index: u32,
    pub present: bool,
    pub end: bool,
    pub run_requested: bool,
    pub name: Optional<Name>,
    #[frame(code)]
    pub state: State,
}
#[weave::interface(tag = u8)]
pub mod operations {
    #[operation(code = 1, request = super::Named, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct Mint;
    #[operation(code = 2, request = super::Named, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct Embark;
    #[operation(code = 3, request = super::Named, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct Debark;
    #[operation(code = 4, request = super::Named, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct State;
    #[operation(code = 5, request = super::Named, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct Ruin;
    #[operation(code = 6, request = super::Instance, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct EmbarkInstance;
    #[operation(code = 7, request = super::Instance, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct DebarkInstance;
    #[operation(code = 8, request = super::Instance, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct RuinInstance;
    #[operation(code = 9, request = super::Instance, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct StateInstance;
    #[operation(code = 10, request = super::Named, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct Task;
    #[operation(code = 11, request = super::Instance, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct ReleaseInstance;
    #[operation(code = 12, request = super::Named, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct Recover;
    #[operation(code = 13, request = super::Publisher, response = super::Outcome<crate::PendId, super::Fail>)]
    pub struct Publisher;
    #[operation(code = 14, request = super::Position, response = super::Outcome<super::Row, super::Fail>)]
    pub struct Observe;
}

#[derive(weave::Contract)]
#[contract(request = crate::value::Request<operations::Request, 1>, response = operations::Response)]
pub struct Call;

pub fn grant(operation: operations::Operation) -> Grant {
    use operations::Operation as Op;
    match operation {
        Op::Mint => Grant::Mint,
        Op::Embark | Op::EmbarkInstance => Grant::Embark,
        Op::Debark | Op::DebarkInstance => Grant::Debark,
        Op::Ruin | Op::RuinInstance | Op::ReleaseInstance => Grant::Ruin,
        Op::Recover => Grant::Recover,
        Op::Observe => Grant::Observe,
        Op::State | Op::StateInstance | Op::Task | Op::Publisher => Grant::State,
    }
}

pub const NAME: &str = "control";
pub const INTERFACE_ID: &str = "shuyra.system.control.v3";

impl Grant {
    pub const fn name(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Mint => "mint",
            Self::Embark => "embark",
            Self::Debark => "debark",
            Self::Ruin => "ruin",
            Self::Observe => "observe",
            Self::Recover => "recover",
        }
    }
}

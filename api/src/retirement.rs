//! Completion is the provider's explicit safe-retirement receipt.
use crate::{TaskId, value::Outcome};
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum State {
    Unseen = 0,
    Pending = 1,
    Settled = 2,
    Failed = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Fail {
    Unknown = 1,
    Denied = 2,
    Busy = 3,
    Failed = 4,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Target {
    pub task: TaskId,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Receipt {
    pub task: TaskId,
    #[frame(code)]
    pub state: State,
}
impl Receipt {
    pub fn settled(&self) -> bool {
        self.state == State::Settled
    }
}
#[weave::interface(tag = u8)]
pub mod operations {
    #[operation(code = 1, request = super::Target, response = super::Outcome<super::Receipt, super::Fail>)]
    pub struct Retire;
    #[operation(code = 2, request = super::Target, response = super::Outcome<super::Receipt, super::Fail>)]
    pub struct Inspect;
}

#[derive(weave::Contract)]
#[contract(request = crate::value::Request<operations::Request, 7>, response = operations::Response)]
pub struct Call;

pub const NAME: &str = "retirement";
pub const INTERFACE_ID: &str = "shuyra.system.retirement.v3";

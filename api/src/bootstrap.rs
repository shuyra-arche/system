//! A startup handoff is accepted only from the transport-authenticated creator.
use crate::TaskId;
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Frame)]
pub struct Frame {
    pub version: u16,
    pub provider: TaskId,
    pub system: TaskId,
}
impl Frame {
    pub fn valid_from(&self, creator: TaskId, sender: TaskId) -> bool {
        self.version == crate::VERSION
            && creator != TaskId::NONE
            && sender == creator
            && self.provider == creator
            && self.system != TaskId::NONE
    }
}

pub const NAME: &str = "bootstrap";
pub const INTERFACE_ID: &str = "shuyra.system.bootstrap.v4";

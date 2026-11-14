//! UnitRoot registry v1, backed by ordinary Operator Panes and explicit Grants.
use super::{Binding, Entry, GrantId, Ops, Scope};
use crate::{Mark, Name, PendId, TaskId, TeamId, control::instance::InstanceId, value::Optional};
use alloc::vec::Vec;
request_id!(AssociationId);

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Registration {
    pub instance: InstanceId,
    pub owner: TaskId,
    pub task: TaskId,
    pub team: TeamId,
    pub pane: Binding,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Phase {
    Preparing = 1,
    Active = 2,
    Closing = 3,
    Retiring = 4,
    Retired = 5,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Root {
    pub registration: Registration,
    pub revision: u64,
    #[frame(code)]
    pub phase: Phase,
}
/// Revision zero registers Preparing; each accepted transition advances once.
/// Exact repeats of an accepted (instance, expected) return its original receipt.
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Advance {
    pub instance: InstanceId,
    pub expected: u64,
    #[frame(code)]
    pub phase: Phase,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Grant {
    pub id: GrantId,
    pub anchor: Entry,
    pub scope: Scope,
    pub ops: Ops,
    /// A delivered coordinate in the actual querying Task's table.
    pub entrance: PendId,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Access {
    pub root: Root,
    #[frame(sequence(prefix = u8, max = 32))]
    pub grants: Vec<Grant>,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Associate {
    pub id: AssociationId,
    pub mark: Mark,
    pub name: Name,
    pub expected: u64,
    /// None unbinds discovery. It never revokes an existing consumer Pend.
    pub instance: Optional<InstanceId>,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Association {
    pub mark: Mark,
    pub name: Name,
    pub revision: u64,
    pub owner: TaskId,
    pub instance: Optional<InstanceId>,
    pub pane: Optional<Entry>,
}
pub const INTERFACE_ID: &str = "shuyra.system.operator.unit.v1";

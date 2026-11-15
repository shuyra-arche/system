//! Instance control v1. IDs select records; the authenticated caller and its
//! delivered endpoint grant determine authority. Task/Team fields are observations.
use crate::{
    TaskId, TeamId,
    value::{Optional, Outcome},
};

request_id!(Sequence);
request_id!(OperationId);

/// A non-reusing sequence in one live System Task. It is not a native Task ID
/// and must not be carried across a Kernel boot. A replacement System has a
/// different issuer and cannot accept the previous issuer's IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstanceId {
    pub system: TaskId,
    pub sequence: Sequence,
}
#[derive(weave::Frame)]
struct RawInstanceId {
    system: TaskId,
    sequence: Sequence,
}
impl weave::Codec for InstanceId {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawInstanceId::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if self.system == TaskId::NONE {
            return None;
        }
        RawInstanceId {
            system: self.system,
            sequence: self.sequence,
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawInstanceId::decode(bytes)?;
        (raw.system != TaskId::NONE).then_some((
            Self {
                system: raw.system,
                sequence: raw.sequence,
            },
            used,
        ))
    }
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
    Interrupted = 8,
}

/// v1 retains ownership with the actual caller; selected-owner transfer is not
/// supported by this interface. Construction still requires declaration policy.
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Create {
    pub id: OperationId,
    pub input: crate::loader::Input,
    pub subject: crate::identity::Subject,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Change {
    pub id: OperationId,
    pub instance: InstanceId,
}
/// Publish a ready instance's UnitRoot under a declaration-selected namespace
/// key. The caller must own the new instance and any existing name association.
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Publication {
    pub id: OperationId,
    pub name: crate::Name,
    pub expected: u64,
    pub instance: Optional<InstanceId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Phase {
    Building = 1,
    Prepared = 2,
    Running = 3,
    Closing = 4,
    Retired = 5,
}

/// Current observation, serialized by System's instance owner. A completed
/// operation receipt is historical; use Inspect to obtain current lifecycle state.
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Observation {
    pub instance: InstanceId,
    #[frame(code)]
    pub phase: Phase,
    pub ready: bool,
    pub task: TaskId,
    pub team: TeamId,
    pub reason: u64,
    pub failure: Optional<weave::Coded<Fail>>,
    pub root: Optional<crate::operator::unit::Root>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum State {
    Pending = 1,
    Completed = 2,
    Failed = 3,
    Cancelled = 4,
}

/// Queryable acceptance/completion record scoped to System and actual caller.
/// Pending survives lost replies. Retire/Cancelled require physical cleanup;
/// Stop completes after native stop admission and does not prove reclamation.
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Receipt {
    pub id: OperationId,
    pub instance: Optional<InstanceId>,
    #[frame(code)]
    pub state: State,
    pub failure: Optional<weave::Coded<Fail>>,
}

#[weave::interface(tag = u8)]
pub mod operations {
    #[operation(code = 1, request = super::Create, response = super::Outcome<super::Receipt, super::Fail>)]
    pub struct Create;
    #[operation(code = 2, request = super::Change, response = super::Outcome<super::Receipt, super::Fail>)]
    pub struct Start;
    #[operation(code = 3, request = super::Change, response = super::Outcome<super::Receipt, super::Fail>)]
    pub struct Stop;
    #[operation(code = 4, request = super::Change, response = super::Outcome<super::Receipt, super::Fail>)]
    pub struct Retire;
    #[operation(code = 5, request = super::InstanceId, response = super::Outcome<super::Observation, super::Fail>)]
    pub struct Inspect;
    #[operation(code = 6, request = super::OperationId, response = super::Outcome<super::Receipt, super::Fail>)]
    pub struct InspectOperation;
    #[operation(code = 7, request = super::OperationId, response = super::Outcome<super::Receipt, super::Fail>)]
    pub struct Cancel;
    #[operation(code = 8, request = super::Publication, response = super::Outcome<super::Receipt, super::Fail>)]
    pub struct Publish;
}

#[derive(weave::Contract)]
#[contract(request = crate::value::Request<operations::Request, 10>, response = operations::Response)]
pub struct Call;

pub const INTERFACE_ID: &str = "shuyra.system.control.instance.v1";

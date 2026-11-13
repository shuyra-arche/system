//! Loader lifecycle and construction, independent of executable bytes.
use crate::{
    Name, PendId, TaskId, TeamId,
    value::{Optional, Outcome},
};
use alloc::vec::Vec;
pub const MAX_ARGS: usize = 64;
pub const MAX_IMAGE: usize = 16 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Fail {
    Unknown = 1,
    BadImage = 2,
    Full = 3,
    NotReady = 4,
    Bad = 5,
    Denied = 6,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum State {
    Unseen = 0,
    Pending = 1,
    Completed = 2,
    Failed = 3,
    Cancelled = 4,
    Closed = 5,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Built {
    pub team: TeamId,
    pub task: TaskId,
    pub resolve: PendId,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Arguments {
    #[frame(sequence(prefix = u8, max = 64))]
    pub values: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitInput {
    pub name: Name,
    pub stack: u64,
    pub arguments: Arguments,
}
#[derive(weave::Frame)]
struct RawUnitInput {
    name: Name,
    stack: u64,
    arguments: Arguments,
}
impl UnitInput {
    pub fn valid(&self) -> bool {
        self.stack <= 1 << 20
    }
}
impl weave::Codec for UnitInput {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawUnitInput::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawUnitInput {
            name: self.name.clone(),
            stack: self.stack,
            arguments: self.arguments.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawUnitInput::decode(bytes)?;
        let value = Self {
            name: raw.name,
            stack: raw.stack,
            arguments: raw.arguments,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageInput {
    pub image: PendId,
    pub offset: u64,
    pub len: u64,
    pub stack: u64,
    pub arguments: Arguments,
}
#[derive(weave::Frame)]
struct RawImageInput {
    image: PendId,
    offset: u64,
    len: u64,
    stack: u64,
    arguments: Arguments,
}
impl ImageInput {
    pub fn valid(&self) -> bool {
        self.image != PendId::NONE
            && self.len != 0
            && self.len <= MAX_IMAGE as u64
            && self.offset.checked_add(self.len).is_some()
            && self.stack <= 1 << 20
    }
}
impl weave::Codec for ImageInput {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawImageInput::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawImageInput {
            image: self.image,
            offset: self.offset,
            len: self.len,
            stack: self.stack,
            arguments: self.arguments.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawImageInput::decode(bytes)?;
        let value = Self {
            image: raw.image,
            offset: raw.offset,
            len: raw.len,
            stack: raw.stack,
            arguments: raw.arguments,
        };
        value.valid().then_some((value, used))
    }
}

union_codec!(Input { ; Unit(UnitInput) = 1, Image(ImageInput) = 2 });
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Build {
    pub id: CreateId,
    pub input: Input,
    pub owner: TaskId,
    pub subject: crate::identity::Subject,
}

request_id!(CreateId);

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Construction {
    #[frame(code)]
    pub state: State,
    pub built: Optional<Built>,
    pub failure: Optional<weave::Coded<Fail>>,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Claim {
    pub task: TaskId,
}

#[weave::interface(tag = u8)]
pub mod operations {
    #[operation(code = 1, request = super::Claim, response = super::Outcome<super::Built, super::Fail>)]
    pub struct Claim;
    #[operation(code = 2, request = super::Build, response = super::Outcome<super::Construction, super::Fail>)]
    pub struct Build;
    #[operation(code = 3, request = super::CreateId, response = super::Outcome<super::Construction, super::Fail>)]
    pub struct Inspect;
    #[operation(code = 4, request = super::CreateId, response = super::Outcome<super::Construction, super::Fail>)]
    pub struct Cancel;
}

#[derive(weave::Contract)]
#[contract(request = crate::value::Request<operations::Request, 4>, response = operations::Response)]
pub struct Call;

pub const NAME: &str = "loader";
pub const INTERFACE_ID: &str = "shuyra.system.loader.v3";

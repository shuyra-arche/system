//! Namespace references, attenuation, publication and observation contracts.
use crate::{
    EntryId, Mark, Name, Path, PendId, TaskId,
    identity::Selector,
    value::{Optional, Outcome, Rights},
};
use alloc::vec::Vec;
use weave::{Codec, EncodedSize};
pub const PANE_CAP: usize = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Fail {
    Unknown = 1,
    NonEmpty = 2,
    NotATile = 3,
    NotAPane = 4,
    Full = 5,
    Dead = 6,
    Bad = 7,
    Denied = 8,
    Unjudged = 9,
    Conflict = 10,
}
union_codec!(EntryTarget { Root = 0; Node(EntryId) = 1 });
union_codec!(Permit { Public = 0, Bound = 1; Identity(Selector) = 2, Opener(EntryId) = 3 });
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub operator: TaskId,
    pub target: EntryTarget,
}
#[derive(weave::Frame)]
struct RawEntry {
    operator: TaskId,
    target: EntryTarget,
}
impl Entry {
    pub fn valid(&self) -> bool {
        self.operator != TaskId::NONE
    }
}
impl weave::Codec for Entry {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawEntry::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawEntry {
            operator: self.operator,
            target: self.target.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawEntry::decode(bytes)?;
        let value = Self {
            operator: raw.operator,
            target: raw.target,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Binding {
    pub entry: Entry,
    pub revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Resource {
    pub binding: Binding,
    pub token: PendId,
    pub rights: Rights,
    pub permit: Permit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ops(u16);
impl Ops {
    pub const PART: Self = Self(1 << 0);
    pub const UNPART: Self = Self(1 << 1);
    pub const BIND: Self = Self(1 << 2);
    pub const REPLACE: Self = Self(1 << 3);
    pub const UNBIND: Self = Self(1 << 4);
    pub const INSPECT: Self = Self(1 << 5);
    pub const GRANT: Self = Self(1 << 6);
    pub const REVOKE: Self = Self(1 << 7);
    pub const FIND: Self = Self(1 << 8);
    pub const CANCEL: Self = Self(1 << 9);
    pub const ALL: Self = Self(0x03ff);
    pub const fn new(bits: u16) -> Option<Self> {
        if bits & !Self::ALL.0 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }
    pub const fn bits(self) -> u16 {
        self.0
    }
    pub fn contains(self, child: Self) -> bool {
        self.0 & child.0 == child.0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}
impl Codec for Ops {
    type Decoded = Self;
    const SIZE: EncodedSize = EncodedSize::Fixed(2);
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        self.0.encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (bits, used) = u16::decode(bytes)?;
        Some((Self::new(bits)?, used))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Names {
    pub names: Vec<Path>,
}
#[derive(weave::Frame)]
struct RawNames {
    #[frame(sequence(prefix = u8, max = 32))]
    names: Vec<Path>,
}
impl Names {
    pub fn valid(&self) -> bool {
        !self.names.is_empty() && self.names.iter().all(|path| !path.as_str().is_empty())
    }
}
impl weave::Codec for Names {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawNames::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawNames {
            names: self.names.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawNames::decode(bytes)?;
        let value = Self { names: raw.names };
        value.valid().then_some((value, used))
    }
}

union_codec!(Scope { Subtree = 0; Names(Names) = 1 });
impl Scope {
    pub fn allows(&self, relative: &str) -> bool {
        match self {
            Self::Subtree => true,
            Self::Names(names) => names.names.iter().any(|name| {
                let prefix = name.as_str();
                relative == prefix
                    || relative
                        .strip_prefix(prefix)
                        .is_some_and(|tail| tail.starts_with('/'))
            }),
        }
    }
    pub fn narrows(&self, child: &Self) -> bool {
        match (self, child) {
            (Self::Subtree, _) => true,
            (Self::Names(_), Self::Subtree) => false,
            (parent, Self::Names(names)) => {
                names.names.iter().all(|name| parent.allows(name.as_str()))
            }
        }
    }

    /// Check a child's scope after moving its anchor below this one.
    /// The empty path keeps the current anchor; component boundaries and the
    /// ordinary path bounds apply to every composed name.
    pub fn narrows_at(&self, relative: &Path, child: &Self) -> bool {
        if relative.as_str().is_empty() {
            return self.narrows(child);
        }
        match child {
            Self::Subtree => self.allows(relative.as_str()),
            Self::Names(names) => names.names.iter().all(|name| {
                let joined = alloc::format!("{}/{}", relative.as_str(), name.as_str());
                Path::new(&joined).is_some() && self.allows(&joined)
            }),
        }
    }
}
request_id!(GrantId);

request_id!(OperationId);

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Source {
    pub task: TaskId,
    pub mark: Mark,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct GrantSpec {
    pub id: GrantId,
    pub parent: Optional<GrantId>,
    pub holder: TaskId,
    pub anchor: Entry,
    pub scope: Scope,
    pub ops: Ops,
    pub permit: Optional<Permit>,
    pub source: Optional<TaskId>,
    pub source_mark: Optional<Mark>,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Named {
    pub name: Name,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct At {
    pub entry: Entry,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Road {
    pub path: Path,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Relative {
    pub base: Entry,
    pub path: Path,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Resolve {
    pub mark: Mark,
    pub name: Name,
}

/// Trusted namespace association. It names a pane, not an imported resource.
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Association {
    pub mark: Mark,
    pub name: Name,
    pub entry: Entry,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Watch {
    pub base: Entry,
    pub path: Path,
    pub endpoint: PendId,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Mutation {
    pub id: OperationId,
    pub entry: Entry,
    pub expected: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Part {
    pub change: Mutation,
    pub name: Name,
    pub permit: Permit,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Bind {
    pub change: Mutation,
    pub name: Name,
    pub resource: PendId,
    pub resource_id: u64,
    pub rights: Rights,
    pub permit: Permit,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Query {
    pub anchor: Entry,
    pub name: Name,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum GrantState {
    Unseen = 0,
    Pending = 1,
    Granted = 2,
    Revoking = 3,
    Closed = 4,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum OperationState {
    Unseen = 0,
    Pending = 1,
    Applied = 2,
    Closed = 3,
    Failed = 4,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct GrantStatus {
    pub state: weave::Coded<GrantState>,
    pub grant: GrantId,
    /// Coordinate in the declared holder's table, never the issuer's table.
    /// Present only after the native entrance grant has been delivered.
    pub entrance: Optional<PendId>,
    pub binding: Optional<Binding>,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct MutationStatus {
    pub state: weave::Coded<OperationState>,
    pub binding: Optional<Binding>,
    pub failure: Optional<weave::Coded<Fail>>,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Listing {
    #[frame(sequence(prefix = u8, max = 32))]
    pub ids: Vec<EntryId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Kind {
    Landed = 1,
    Rebound = 2,
    Parted = 3,
    Trimmed = 4,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Event {
    pub sequence: u64,
    pub kind: weave::Coded<Kind>,
    pub path: Path,
    pub entry: EntryId,
    pub owner: TaskId,
}

#[weave::interface(tag = u8)]
pub mod operations {
    #[operation(code = 3, request = super::At, response = super::Outcome<super::Resource, super::Fail>)]
    pub struct Find;
    #[operation(code = 5, request = super::At, response = super::Outcome<super::Listing, super::Fail>)]
    pub struct List;
    #[operation(code = 6, request = super::At, response = super::Outcome<super::Name, super::Fail>)]
    pub struct Name;
    #[operation(code = 7, request = super::Road, response = super::Outcome<super::Entry, super::Fail>)]
    pub struct Seek;
    #[operation(code = 8, request = super::Watch, response = super::Outcome<super::Path, super::Fail>)]
    pub struct Watch;
    #[operation(code = 9, request = super::Resolve, response = super::Outcome<super::Entry, super::Fail>)]
    pub struct Resolve;
    #[operation(code = 10, request = super::Relative, response = super::Outcome<super::Entry, super::Fail>)]
    pub struct Relative;
    #[operation(code = 11, request = super::Watch, response = super::Outcome<super::Path, super::Fail>)]
    pub struct WatchFrom;
}

#[derive(weave::Contract)]
#[contract(request = crate::value::Request<operations::Request, 3>, response = operations::Response)]
pub struct Call;

pub mod authority {
    use super::*;
    #[weave::interface(tag = u8)]
    pub mod operations {
        #[operation(code = 1, request = super::Part, response = super::Outcome<super::Binding, super::Fail>)]
        pub struct Part;
        #[operation(code = 2, request = super::Bind, response = super::Outcome<super::Binding, super::Fail>)]
        pub struct Bind;
        #[operation(code = 3, request = super::Bind, response = super::Outcome<super::Binding, super::Fail>)]
        pub struct Replace;
        #[operation(code = 4, request = super::Mutation, response = super::Outcome<super::MutationStatus, super::Fail>)]
        pub struct Unbind;
        #[operation(code = 5, request = super::GrantSpec, response = super::Outcome<super::GrantStatus, super::Fail>)]
        pub struct Grant;
        #[operation(code = 6, request = super::GrantId, response = super::Outcome<super::GrantStatus, super::Fail>)]
        pub struct Inspect;
        #[operation(code = 7, request = super::GrantId, response = super::Outcome<super::GrantStatus, super::Fail>)]
        pub struct Revoke;
        #[operation(code = 8, request = super::Mutation, response = super::Outcome<super::MutationStatus, super::Fail>)]
        pub struct Unpart;
        #[operation(code = 9, request = super::Query, response = super::Outcome<super::Binding, super::Fail>)]
        pub struct Find;
        #[operation(code = 10, request = super::At, response = super::Outcome<super::Resource, super::Fail>)]
        pub struct Read;
        #[operation(code = 11, request = super::OperationId, response = super::Outcome<super::MutationStatus, super::Fail>)]
        pub struct Cancel;
        #[operation(code = 12, request = super::OperationId, response = super::Outcome<super::MutationStatus, super::Fail>)]
        pub struct InspectOperation;
        #[operation(code = 13, request = super::Association, response = super::Outcome<(), super::Fail>)]
        pub struct Associate;
        #[operation(code = 14, request = super::Resolve, response = super::Outcome<super::Entry, super::Fail>)]
        pub struct ResolveNamespace;
    }

    #[derive(weave::Contract)]
    #[contract(request = crate::value::Request<operations::Request, 6>, response = operations::Response)]
    pub struct Call;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wired {
    pub authority: TaskId,
    pub resolve: PendId,
    pub matches: PendId,
    pub same: PendId,
}
#[derive(weave::Frame)]
struct RawWired {
    authority: TaskId,
    resolve: PendId,
    matches: PendId,
    same: PendId,
}
impl Wired {
    pub fn valid(&self) -> bool {
        self.authority != TaskId::NONE
            && self.resolve != PendId::NONE
            && self.matches != PendId::NONE
            && self.same != PendId::NONE
            && self.resolve != self.matches
            && self.resolve != self.same
            && self.matches != self.same
    }
}
impl weave::Codec for Wired {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawWired::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawWired {
            authority: self.authority,
            resolve: self.resolve,
            matches: self.matches,
            same: self.same,
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawWired::decode(bytes)?;
        let value = Self {
            authority: raw.authority,
            resolve: raw.resolve,
            matches: raw.matches,
            same: raw.same,
        };
        value.valid().then_some((value, used))
    }
}

pub mod setup {
    use super::*;
    #[weave::interface(tag = u8)]
    pub mod operations {
        #[operation(code = 3, request = super::Wired, response = super::Outcome<(), super::Fail>)]
        pub struct Wired;
        #[operation(code = 7, request = (), response = super::Outcome<(), super::Fail>)]
        pub struct Root;
    }

    #[derive(weave::Contract)]
    #[contract(request = crate::value::Request<operations::Request, 8>, response = operations::Response)]
    pub struct Call;
}

pub const NAME: &str = "operator";
pub const INTERFACE_ID: &str = "shuyra.system.operator.v3";

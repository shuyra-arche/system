//! Identity authority remains part of every principal and coalition reference.
mod paging;
mod subject;
pub use paging::{CoalitionPage, Cursor, PageTarget, PrincipalPage};
pub use subject::{
    Binding, CoalitionId, CoalitionSet, IdentityObject, PrincipalId, Selector, Subject,
};

use crate::{
    Name, TaskId,
    value::{Optional, Outcome},
};
pub const MAX_ACTIVE_COALITIONS: usize = 16;
pub const MAX_PAGE_ITEMS: usize = 16;
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Fail {
    Bad = 1,
    UnknownPrincipal = 2,
    UnknownCoalition = 3,
    WrongAuthority = 4,
    Denied = 5,
    NotManager = 6,
    NotNarrower = 7,
    NotEligible = 8,
    Full = 9,
    Changed = 10,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restriction {
    pub parent: TaskId,
    pub subject: Subject,
}
#[derive(weave::Frame)]
struct RawRestriction {
    parent: TaskId,
    subject: Subject,
}
impl Restriction {
    pub fn valid(&self) -> bool {
        self.parent != TaskId::NONE
    }
}
impl weave::Codec for Restriction {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawRestriction::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawRestriction {
            parent: self.parent,
            subject: self.subject.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawRestriction::decode(bytes)?;
        let value = Self {
            parent: raw.parent,
            subject: raw.subject,
        };
        value.valid().then_some((value, used))
    }
}

union_codec!(Install { ; Authorized(Subject) = 0, Inherit(Task) = 1, Restrict(Restriction) = 2, Derive(PrincipalId) = 3 });
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Match {
    Yes = 0,
    No = 1,
    Unbound = 2,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct MatchReply {
    pub value: weave::Coded<Match>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorEntryRef {
    pub operator: TaskId,
    pub root: bool,
    pub id: crate::EntryId,
    pub revision: u64,
}
#[derive(weave::Frame)]
struct RawOperatorEntryRef {
    operator: TaskId,
    root: bool,
    id: crate::EntryId,
    revision: u64,
}
impl OperatorEntryRef {
    pub fn valid(&self) -> bool {
        self.operator != TaskId::NONE && (!self.root || self.id == crate::EntryId::NONE)
    }
}
impl weave::Codec for OperatorEntryRef {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawOperatorEntryRef::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawOperatorEntryRef {
            operator: self.operator,
            root: self.root,
            id: self.id,
            revision: self.revision,
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawOperatorEntryRef::decode(bytes)?;
        let value = Self {
            operator: raw.operator,
            root: raw.root,
            id: raw.id,
            revision: raw.revision,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum InstallState {
    Unseen = 0,
    Pending = 1,
    Installed = 2,
    Retiring = 3,
    Retired = 4,
    Cancelled = 5,
    Failed = 6,
    Closed = 7,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct InstallStatus {
    pub state: weave::Coded<InstallState>,
    pub binding: Optional<Binding>,
    pub failure: Optional<weave::Coded<Fail>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub task: TaskId,
}
#[derive(weave::Frame)]
struct RawTask {
    task: TaskId,
}
impl Task {
    pub fn valid(&self) -> bool {
        self.task != TaskId::NONE
    }
}
impl weave::Codec for Task {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawTask::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawTask { task: self.task }.encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawTask::decode(bytes)?;
        let value = Self { task: raw.task };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matches {
    pub task: TaskId,
    pub selector: Selector,
}
#[derive(weave::Frame)]
struct RawMatches {
    task: TaskId,
    selector: Selector,
}
impl Matches {
    pub fn valid(&self) -> bool {
        self.task != TaskId::NONE
    }
}
impl weave::Codec for Matches {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawMatches::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawMatches {
            task: self.task,
            selector: self.selector.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawMatches::decode(bytes)?;
        let value = Self {
            task: raw.task,
            selector: raw.selector,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Same {
    pub left: TaskId,
    pub right: TaskId,
}
#[derive(weave::Frame)]
struct RawSame {
    left: TaskId,
    right: TaskId,
}
impl Same {
    pub fn valid(&self) -> bool {
        self.left != TaskId::NONE && self.right != TaskId::NONE
    }
}
impl weave::Codec for Same {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawSame::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawSame {
            left: self.left,
            right: self.right,
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawSame::decode(bytes)?;
        let value = Self {
            left: raw.left,
            right: raw.right,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heir {
    pub parent: PrincipalId,
    pub child: PrincipalId,
}
#[derive(weave::Frame)]
struct RawHeir {
    parent: PrincipalId,
    child: PrincipalId,
}
impl Heir {
    pub fn valid(&self) -> bool {
        self.parent.authority == self.child.authority
    }
}
impl weave::Codec for Heir {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawHeir::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawHeir {
            parent: self.parent,
            child: self.child,
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawHeir::decode(bytes)?;
        let value = Self {
            parent: raw.parent,
            child: raw.child,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Amid {
    pub principal: PrincipalId,
    pub coalition: CoalitionId,
}
#[derive(weave::Frame)]
struct RawAmid {
    principal: PrincipalId,
    coalition: CoalitionId,
}
impl Amid {
    pub fn valid(&self) -> bool {
        self.principal.authority == self.coalition.authority
    }
}
impl weave::Codec for Amid {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawAmid::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawAmid {
            principal: self.principal,
            coalition: self.coalition,
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawAmid::decode(bytes)?;
        let value = Self {
            principal: raw.principal,
            coalition: raw.coalition,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Members {
    pub coalition: CoalitionId,
    pub cursor: Optional<Cursor>,
}
#[derive(weave::Frame)]
struct RawMembers {
    coalition: CoalitionId,
    cursor: Optional<Cursor>,
}
impl Members {
    pub fn valid(&self) -> bool {
        self.cursor
            .0
            .as_ref()
            .is_none_or(|cursor| cursor.target == PageTarget::Members(self.coalition))
    }
}
impl weave::Codec for Members {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawMembers::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawMembers {
            coalition: self.coalition,
            cursor: self.cursor.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawMembers::decode(bytes)?;
        let value = Self {
            coalition: raw.coalition,
            cursor: raw.cursor,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memberships {
    pub principal: PrincipalId,
    pub cursor: Optional<Cursor>,
}
#[derive(weave::Frame)]
struct RawMemberships {
    principal: PrincipalId,
    cursor: Optional<Cursor>,
}
impl Memberships {
    pub fn valid(&self) -> bool {
        self.cursor
            .0
            .as_ref()
            .is_none_or(|cursor| cursor.target == PageTarget::Memberships(self.principal))
    }
}
impl weave::Codec for Memberships {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawMemberships::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawMemberships {
            principal: self.principal,
            cursor: self.cursor.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawMemberships::decode(bytes)?;
        let value = Self {
            principal: raw.principal,
            cursor: raw.cursor,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bind {
    pub task: TaskId,
    pub install: Install,
}
#[derive(weave::Frame)]
struct RawBind {
    task: TaskId,
    install: Install,
}
impl Bind {
    pub fn valid(&self) -> bool {
        self.task != TaskId::NONE
    }
}
impl weave::Codec for Bind {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawBind::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawBind {
            task: self.task,
            install: self.install.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawBind::decode(bytes)?;
        let value = Self {
            task: raw.task,
            install: raw.install,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activate {
    pub task: TaskId,
    pub coalitions: CoalitionSet,
}
#[derive(weave::Frame)]
struct RawActivate {
    task: TaskId,
    coalitions: CoalitionSet,
}
impl Activate {
    pub fn valid(&self) -> bool {
        self.task != TaskId::NONE && !self.coalitions.as_slice().is_empty()
    }
}
impl weave::Codec for Activate {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawActivate::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawActivate {
            task: self.task,
            coalitions: self.coalitions.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawActivate::decode(bytes)?;
        let value = Self {
            task: raw.task,
            coalitions: raw.coalitions,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallRequest {
    pub id: InstallId,
    pub task: TaskId,
    pub install: Install,
}
#[derive(weave::Frame)]
struct RawInstallRequest {
    id: InstallId,
    task: TaskId,
    install: Install,
}
impl InstallRequest {
    pub fn valid(&self) -> bool {
        self.task != TaskId::NONE
    }
}
impl weave::Codec for InstallRequest {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawInstallRequest::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawInstallRequest {
            id: self.id,
            task: self.task,
            install: self.install.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawInstallRequest::decode(bytes)?;
        let value = Self {
            id: raw.id,
            task: raw.task,
            install: raw.install,
        };
        value.valid().then_some((value, used))
    }
}

request_id!(InstallId);

request_id!(FoundId);

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Associate {
    pub object: IdentityObject,
    pub name: Name,
    pub entry: OperatorEntryRef,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Reference {
    pub object: IdentityObject,
    pub name: Name,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Export {
    pub object: IdentityObject,
    pub alias: Name,
    pub binding: Name,
    pub entry: OperatorEntryRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Grant {
    Resolve = 1,
    Matches = 2,
    Same = 3,
    Sire = 4,
    Heir = 5,
    Amid = 6,
    Members = 7,
    Memberships = 8,
    Adopt = 9,
    Waive = 10,
    Restrict = 11,
    Derive = 12,
    Found = 13,
    Admit = 14,
    Expel = 15,
    Bind = 16,
    Unbind = 17,
    Activate = 18,
    Install = 19,
    Reference = 20,
}
#[weave::interface(tag = u8)]
pub mod operations {
    #[operation(code = 1, request = super::Task, response = super::Outcome<super::Optional<super::Binding>, super::Fail>)]
    pub struct Resolve;
    #[operation(code = 2, request = super::Matches, response = super::Outcome<super::MatchReply, super::Fail>)]
    pub struct Matches;
    #[operation(code = 3, request = super::Same, response = super::Outcome<bool, super::Fail>)]
    pub struct Same;
    #[operation(code = 4, request = super::PrincipalId, response = super::Outcome<super::Optional<super::PrincipalId>, super::Fail>)]
    pub struct Sire;
    #[operation(code = 5, request = super::Heir, response = super::Outcome<bool, super::Fail>)]
    pub struct Heir;
    #[operation(code = 6, request = super::Amid, response = super::Outcome<bool, super::Fail>)]
    pub struct Amid;
    #[operation(code = 7, request = super::Members, response = super::Outcome<super::PrincipalPage, super::Fail>)]
    pub struct Members;
    #[operation(code = 8, request = super::Memberships, response = super::Outcome<super::CoalitionPage, super::Fail>)]
    pub struct Memberships;
    #[operation(code = 9, request = super::Subject, response = super::Outcome<super::Binding, super::Fail>)]
    pub struct Adopt;
    #[operation(code = 10, request = (), response = super::Outcome<(), super::Fail>)]
    pub struct Waive;
    #[operation(code = 11, request = super::Subject, response = super::Outcome<super::Binding, super::Fail>)]
    pub struct Restrict;
    #[operation(code = 12, request = super::PrincipalId, response = super::Outcome<super::PrincipalId, super::Fail>)]
    pub struct Derive;
    #[operation(code = 13, request = super::FoundId, response = super::Outcome<super::CoalitionId, super::Fail>)]
    pub struct Found;
    #[operation(code = 14, request = super::Amid, response = super::Outcome<(), super::Fail>)]
    pub struct Admit;
    #[operation(code = 15, request = super::Amid, response = super::Outcome<(), super::Fail>)]
    pub struct Expel;
    #[operation(code = 16, request = super::Bind, response = super::Outcome<super::Binding, super::Fail>)]
    pub struct Bind;
    #[operation(code = 17, request = super::Task, response = super::Outcome<(), super::Fail>)]
    pub struct Unbind;
    #[operation(code = 18, request = super::Activate, response = super::Outcome<super::Binding, super::Fail>)]
    pub struct Activate;
    #[operation(code = 19, request = super::InstallRequest, response = super::Outcome<super::InstallStatus, super::Fail>)]
    pub struct Install;
    #[operation(code = 20, request = super::Reference, response = super::Outcome<super::Optional<super::OperatorEntryRef>, super::Fail>)]
    pub struct Reference;
    #[operation(code = 21, request = super::InstallId, response = super::Outcome<super::InstallStatus, super::Fail>)]
    pub struct Inspect;
    #[operation(code = 22, request = super::InstallId, response = super::Outcome<super::InstallStatus, super::Fail>)]
    pub struct Cancel;
    #[operation(code = 23, request = super::InstallId, response = super::Outcome<super::InstallStatus, super::Fail>)]
    pub struct Retire;
    #[operation(code = 24, request = super::Associate, response = super::Outcome<(), super::Fail>)]
    pub struct Associate;
}

#[derive(weave::Contract)]
#[contract(request = crate::value::Request<operations::Request, 2>, response = operations::Response)]
pub struct Call;

pub fn grant(operation: operations::Operation) -> Grant {
    use operations::Operation as Op;
    match operation {
        Op::Resolve => Grant::Resolve,
        Op::Matches => Grant::Matches,
        Op::Same => Grant::Same,
        Op::Sire => Grant::Sire,
        Op::Heir => Grant::Heir,
        Op::Amid => Grant::Amid,
        Op::Members => Grant::Members,
        Op::Memberships => Grant::Memberships,
        Op::Adopt => Grant::Adopt,
        Op::Waive => Grant::Waive,
        Op::Restrict => Grant::Restrict,
        Op::Derive => Grant::Derive,
        Op::Found => Grant::Found,
        Op::Admit => Grant::Admit,
        Op::Expel => Grant::Expel,
        Op::Bind => Grant::Bind,
        Op::Unbind => Grant::Unbind,
        Op::Activate => Grant::Activate,
        Op::Install => Grant::Install,
        Op::Reference => Grant::Reference,
        Op::Inspect => Grant::Install,
        Op::Cancel => Grant::Install,
        Op::Retire => Grant::Install,
        Op::Associate => Grant::Install,
    }
}

pub const NAME: &str = "identity";
pub const INTERFACE_ID: &str = "shuyra.system.identity.v2";

impl Grant {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::Matches => "matches",
            Self::Same => "same",
            Self::Sire => "sire",
            Self::Heir => "heir",
            Self::Amid => "amid",
            Self::Members => "members",
            Self::Memberships => "memberships",
            Self::Adopt => "adopt",
            Self::Waive => "waive",
            Self::Restrict => "restrict",
            Self::Derive => "derive",
            Self::Found => "found",
            Self::Admit => "admit",
            Self::Expel => "expel",
            Self::Bind => "bind",
            Self::Unbind => "unbind",
            Self::Activate => "activate",
            Self::Install => "install",
            Self::Reference => "reference",
        }
    }
}

// Trusted export has its own endpoint, separate from ordinary Install grants.
pub mod export {
    #[weave::interface(tag = u8)]
    pub mod operations {
        #[operation(code = 1, request = super::super::Export, response = super::super::Outcome<(), super::super::Fail>)]
        pub struct Export;
    }
    #[derive(weave::Contract)]
    #[contract(request = crate::value::Request<operations::Request, 9>, response = operations::Response)]
    pub struct Call;
}

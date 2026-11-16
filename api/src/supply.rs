//! Resource leases carry explicit native Kernel rights and recipient-local Pends.
use crate::{
    Mark, PendId, TaskId,
    value::{Optional, Outcome, Rights},
};
use alloc::string::String;
use alloc::vec::Vec;
pub const MAX_DESCRIPTION: usize = 128;
pub const MAX_LEASES: usize = 256;
pub const MAX_INSTALLATIONS: usize = 256;
request_id!(LeaseId);
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum State {
    Unseen = 0,
    Acquiring = 1,
    Granted = 2,
    Releasing = 3,
    Released = 4,
    Closed = 5,
    Bad = 6,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum Fail {
    Unknown = 1,
    Bad = 2,
    Denied = 3,
    Conflict = 4,
    Full = 5,
    Pending = 6,
    SourceLost = 7,
    NotReady = 8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub name: String,
}
#[derive(weave::Frame)]
struct RawKey {
    #[frame(text(prefix = u8, max = 128))]
    name: String,
}
impl Key {
    pub fn valid(&self) -> bool {
        !self.name.is_empty() && !self.name.chars().any(char::is_control)
    }
}
impl weave::Codec for Key {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawKey::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawKey {
            name: self.name.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawKey::decode(bytes)?;
        let value = Self { name: raw.name };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub source: Key,
    pub resource: Key,
    pub slot: Mark,
    pub rights: Rights,
}
#[derive(weave::Frame)]
struct RawBinding {
    source: Key,
    resource: Key,
    slot: Mark,
    rights: Rights,
}
impl Binding {
    pub fn valid(&self) -> bool {
        self.slot != Mark::NONE && self.rights.0.operations.words()[1] != 0
    }
}
impl weave::Codec for Binding {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawBinding::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawBinding {
            source: self.source.clone(),
            resource: self.resource.clone(),
            slot: self.slot,
            rights: self.rights,
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawBinding::decode(bytes)?;
        let value = Self {
            source: raw.source,
            resource: raw.resource,
            slot: raw.slot,
            rights: raw.rights,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Acquire {
    pub lease: LeaseId,
    pub target: TaskId,
    pub binding: Binding,
}
#[derive(weave::Frame)]
struct RawAcquire {
    lease: LeaseId,
    target: TaskId,
    binding: Binding,
}
impl Acquire {
    pub fn valid(&self) -> bool {
        self.target != TaskId::NONE
    }
}
impl weave::Codec for Acquire {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawAcquire::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawAcquire {
            lease: self.lease,
            target: self.target,
            binding: self.binding.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawAcquire::decode(bytes)?;
        let value = Self {
            lease: raw.lease,
            target: raw.target,
            binding: raw.binding,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Lease {
    pub lease: LeaseId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub token: PendId,
    pub description: Vec<u8>,
}
#[derive(weave::Frame)]
struct RawDelivery {
    token: PendId,
    #[frame(bytes(prefix = u16, max = 128))]
    description: Vec<u8>,
}
impl Delivery {
    pub fn valid(&self) -> bool {
        self.token != PendId::NONE
    }
}
impl weave::Codec for Delivery {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawDelivery::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawDelivery {
            token: self.token,
            description: self.description.clone(),
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawDelivery::decode(bytes)?;
        let value = Self {
            token: raw.token,
            description: raw.description,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Reply {
    #[frame(code)]
    pub state: State,
    pub delivery: Optional<Delivery>,
}
// Caller-scoped registration identity. Retained after withdrawal; never reused.
request_id!(RegistrationId);
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Register {
    pub id: RegistrationId,
    /// Must equal the actual caller's trusted Unit name.
    pub source: Key,
    pub resource: Key,
    /// Already granted to System; the Kernel lender must be the actual caller.
    pub offered: PendId,
    /// Maximum recipient rights. The offered branch must permit delegation.
    pub maximum: Rights,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum SourceState {
    Active = 1,
    Closing = 2,
    Closed = 3,
    /// Withdraw won admission; no offered grant was accepted.
    Cancelled = 4,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Frame)]
pub struct Registration {
    pub id: RegistrationId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Frame)]
pub struct SourceStatus {
    pub id: RegistrationId,
    #[frame(code)]
    pub state: SourceState,
}

// Separate from consumer leases, registrations and Instance operation IDs.
request_id!(InstallId);
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Install {
    pub id: InstallId,
    pub source: RegistrationId,
    pub instance: crate::control::instance::InstanceId,
    /// Exact Task from the caller's Prepared observation; System revalidates it.
    pub target: TaskId,
    pub slot: Mark,
    pub rights: Rights,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Frame)]
pub struct Installation {
    pub id: InstallId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, weave::Code)]
#[repr(u8)]
pub enum InstallState {
    Pending = 1,
    Granted = 2,
    Releasing = 3,
    Released = 4,
    Cancelled = 5,
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct InstallStatus {
    pub id: InstallId,
    #[frame(code)]
    pub state: InstallState,
    pub instance: Optional<crate::control::instance::InstanceId>,
    pub target: TaskId,
    pub slot: Mark,
    /// Recipient-local token, available only while Granted; never caller-local.
    pub delivery: Optional<Delivery>,
    pub failure: Optional<weave::Coded<Fail>>,
}
impl InstallStatus {
    pub fn cancelled(id: InstallId) -> Self {
        Self {
            id,
            state: InstallState::Cancelled,
            instance: Optional(None),
            target: TaskId::NONE,
            slot: Mark::NONE,
            delivery: Optional(None),
            failure: Optional(None),
        }
    }
}

#[weave::interface(tag = u8)]
pub mod operations {
    #[operation(code = 1, request = super::Acquire, response = super::Outcome<super::Reply, super::Fail>)]
    pub struct Acquire;
    #[operation(code = 2, request = super::Lease, response = super::Outcome<super::Reply, super::Fail>)]
    pub struct Inspect;
    #[operation(code = 3, request = super::Lease, response = super::Outcome<super::Reply, super::Fail>)]
    pub struct Release;
    #[operation(code = 4, request = super::Lease, response = super::Outcome<super::Reply, super::Fail>)]
    pub struct Cancel;
    #[operation(code = 5, request = super::Register, response = super::Outcome<super::SourceStatus, super::Fail>)]
    pub struct Register;
    #[operation(code = 6, request = super::Registration, response = super::Outcome<super::SourceStatus, super::Fail>)]
    pub struct InspectSource;
    #[operation(code = 7, request = super::Registration, response = super::Outcome<super::SourceStatus, super::Fail>)]
    pub struct Withdraw;
    #[operation(code = 8, request = super::Install, response = super::Outcome<super::InstallStatus, super::Fail>)]
    pub struct Install;
    #[operation(code = 9, request = super::Installation, response = super::Outcome<super::InstallStatus, super::Fail>)]
    pub struct InspectInstall;
    #[operation(code = 10, request = super::Installation, response = super::Outcome<super::InstallStatus, super::Fail>)]
    pub struct CancelInstall;
}

#[derive(weave::Contract)]
#[contract(request = crate::value::Request<operations::Request, 5>, response = operations::Response)]
pub struct Call;
#[derive(Clone, Copy, Debug, Default)]
pub struct LeaseIds {
    next: u64,
}
impl LeaseIds {
    pub fn reserve(&mut self) -> Option<LeaseId> {
        self.next = self.next.checked_add(1)?;
        LeaseId::new(self.next)
    }
}

pub const NAME: &str = "supply";
pub const INTERFACE_ID: &str = "shuyra.system.supply.v4";

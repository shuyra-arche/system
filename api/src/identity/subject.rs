//! Principal authority, active selections and identity bindings.
use super::MAX_ACTIVE_COALITIONS;
use crate::TaskId;
use weave::{Codec, EncodedSize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrincipalId {
    pub authority: TaskId,
    pub slot: u64,
}
#[derive(weave::Frame)]
struct RawPrincipalId {
    authority: TaskId,
    slot: u64,
}
impl PrincipalId {
    pub fn valid(&self) -> bool {
        self.authority != TaskId::NONE
    }
}
impl weave::Codec for PrincipalId {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawPrincipalId::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawPrincipalId {
            authority: self.authority,
            slot: self.slot,
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawPrincipalId::decode(bytes)?;
        let value = Self {
            authority: raw.authority,
            slot: raw.slot,
        };
        value.valid().then_some((value, used))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoalitionId {
    pub authority: TaskId,
    pub slot: u64,
}
#[derive(weave::Frame)]
struct RawCoalitionId {
    authority: TaskId,
    slot: u64,
}
impl CoalitionId {
    pub fn valid(&self) -> bool {
        self.authority != TaskId::NONE
    }
}
impl weave::Codec for CoalitionId {
    type Decoded = Self;
    const SIZE: weave::EncodedSize = RawCoalitionId::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.valid() {
            return None;
        }
        RawCoalitionId {
            authority: self.authority,
            slot: self.slot,
        }
        .encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = RawCoalitionId::decode(bytes)?;
        let value = Self {
            authority: raw.authority,
            slot: raw.slot,
        };
        value.valid().then_some((value, used))
    }
}

impl PrincipalId {
    pub const EMPTY: Self = Self {
        authority: TaskId::NONE,
        slot: 0,
    };
}
impl CoalitionId {
    pub const EMPTY: Self = Self {
        authority: TaskId::NONE,
        slot: 0,
    };
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
struct CoalitionData {
    count: u8,
    #[frame(sequence(count = count, fill = CoalitionId::EMPTY))]
    items: [CoalitionId; MAX_ACTIVE_COALITIONS],
}

/// Canonical, unique active selections in one authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoalitionSet(CoalitionData);
impl CoalitionSet {
    pub fn new(values: &[CoalitionId]) -> Option<Self> {
        if values.len() > MAX_ACTIVE_COALITIONS {
            return None;
        }
        let mut items = [CoalitionId::EMPTY; MAX_ACTIVE_COALITIONS];
        items[..values.len()].clone_from_slice(values);
        items[..values.len()].sort_by_key(|id| (id.authority.get(), id.slot));
        if items[..values.len()]
            .windows(2)
            .any(|pair| pair[0] == pair[1])
            || values
                .iter()
                .any(|id| id.authority == TaskId::NONE || id.authority != values[0].authority)
        {
            return None;
        }
        Some(Self(CoalitionData {
            count: values.len() as u8,
            items,
        }))
    }
    pub fn len(&self) -> usize {
        self.0.count as usize
    }
    pub fn is_empty(&self) -> bool {
        self.0.count == 0
    }
    pub fn iter(&self) -> impl Iterator<Item = CoalitionId> + '_ {
        self.as_slice().iter().copied()
    }
    pub fn contains(&self, id: CoalitionId) -> bool {
        self.as_slice().binary_search(&id).is_ok()
    }
    pub fn is_subset(&self, other: &Self) -> bool {
        self.iter().all(|id| other.contains(id))
    }
    pub fn remove(&mut self, id: CoalitionId) {
        self.retain(|value| value != id);
    }
    pub fn retain(&mut self, mut keep: impl FnMut(CoalitionId) -> bool) {
        let mut count = 0;
        for at in 0..self.len() {
            if keep(self.0.items[at]) {
                self.0.items[count] = self.0.items[at];
                count += 1;
            }
        }
        self.0.items[count..].fill(CoalitionId::EMPTY);
        self.0.count = count as u8;
    }
    pub fn as_slice(&self) -> &[CoalitionId] {
        &self.0.items[..self.0.count as usize]
    }
}
impl Codec for CoalitionSet {
    type Decoded = Self;
    const SIZE: EncodedSize = CoalitionData::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        self.0.encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = CoalitionData::decode(bytes)?;
        let values = &raw.items[..raw.count as usize];
        if values.windows(2).any(|pair| pair[0].slot >= pair[1].slot) {
            return None;
        }
        Some((Self::new(values)?, used))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
struct SubjectData {
    principal: PrincipalId,
    coalitions: CoalitionSet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject(SubjectData);
impl Subject {
    pub fn new(principal: PrincipalId, coalitions: &[CoalitionId]) -> Option<Self> {
        if principal.authority == TaskId::NONE
            || coalitions
                .iter()
                .any(|id| id.authority != principal.authority)
        {
            return None;
        }
        Some(Self(SubjectData {
            principal,
            coalitions: CoalitionSet::new(coalitions)?,
        }))
    }
    pub fn principal(&self) -> &PrincipalId {
        &self.0.principal
    }
    pub fn coalitions(&self) -> &[CoalitionId] {
        self.0.coalitions.as_slice()
    }
    /// Remove selections without changing their authority or canonical order.
    pub fn retain_coalitions(&mut self, keep: impl FnMut(CoalitionId) -> bool) {
        self.0.coalitions.retain(keep);
    }
}
impl Codec for Subject {
    type Decoded = Self;
    const SIZE: EncodedSize = SubjectData::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        self.0.encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = SubjectData::decode(bytes)?;
        Some((Self::new(raw.principal, raw.coalitions.as_slice())?, used))
    }
}
union_codec!(Selector { ; Exact(PrincipalId) = 0, DescendantOf(PrincipalId) = 1, MemberOf(CoalitionId) = 2 });
union_codec!(IdentityObject { ; Principal(PrincipalId) = 0, Coalition(CoalitionId) = 1 });
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Binding {
    pub origin: Subject,
    pub current: Subject,
}

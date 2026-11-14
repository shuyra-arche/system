//! Ordered, bounded identity pages and revision-qualified cursors.
use super::{CoalitionId, MAX_PAGE_ITEMS, PrincipalId};
use crate::{TaskId, value::Optional};
use weave::{Codec, EncodedSize};

union_codec!(PageTarget { ; Members(CoalitionId) = 0, Memberships(PrincipalId) = 1 });
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
pub struct Cursor {
    pub target: PageTarget,
    pub revision: u64,
    pub after: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
struct PrincipalPageData {
    #[frame(sequence(prefix = u8, max = 16))]
    pub items: alloc::vec::Vec<PrincipalId>,
    pub next: Optional<Cursor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrincipalPage(PrincipalPageData);
impl PrincipalPage {
    pub fn new(values: &[PrincipalId], next: Option<Cursor>) -> Option<Self> {
        if values.len() > MAX_PAGE_ITEMS
            || values.windows(2).any(|pair| pair[0] >= pair[1])
            || values
                .iter()
                .any(|id| id.authority == TaskId::NONE || id.authority != values[0].authority)
        {
            return None;
        }
        if let Some(cursor) = &next {
            let PageTarget::Members(target) = &cursor.target else {
                return None;
            };
            let last = values.last()?;
            if target.authority == TaskId::NONE
                || last.authority != target.authority
                || last.slot != cursor.after
            {
                return None;
            }
        }
        let mut items = alloc::vec::Vec::new();
        items.try_reserve_exact(values.len()).ok()?;
        items.extend_from_slice(values);
        Some(Self(PrincipalPageData {
            items,
            next: Optional(next),
        }))
    }
    pub fn as_slice(&self) -> &[PrincipalId] {
        &self.0.items
    }
    pub fn next(&self) -> Option<&Cursor> {
        self.0.next.0.as_ref()
    }
}
impl Codec for PrincipalPage {
    type Decoded = Self;
    const SIZE: EncodedSize = PrincipalPageData::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        self.0.encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = PrincipalPageData::decode(bytes)?;
        Some((Self::new(&raw.items, raw.next.0)?, used))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, weave::Frame)]
struct CoalitionPageData {
    #[frame(sequence(prefix = u8, max = 16))]
    pub items: alloc::vec::Vec<CoalitionId>,
    pub next: Optional<Cursor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoalitionPage(CoalitionPageData);
impl CoalitionPage {
    pub fn new(values: &[CoalitionId], next: Option<Cursor>) -> Option<Self> {
        if values.len() > MAX_PAGE_ITEMS
            || values.windows(2).any(|pair| pair[0] >= pair[1])
            || values
                .iter()
                .any(|id| id.authority == TaskId::NONE || id.authority != values[0].authority)
        {
            return None;
        }
        if let Some(cursor) = &next {
            let PageTarget::Memberships(target) = &cursor.target else {
                return None;
            };
            let last = values.last()?;
            if target.authority == TaskId::NONE
                || last.authority != target.authority
                || last.slot != cursor.after
            {
                return None;
            }
        }
        let mut items = alloc::vec::Vec::new();
        items.try_reserve_exact(values.len()).ok()?;
        items.extend_from_slice(values);
        Some(Self(CoalitionPageData {
            items,
            next: Optional(next),
        }))
    }
    pub fn as_slice(&self) -> &[CoalitionId] {
        &self.0.items
    }
    pub fn next(&self) -> Option<&Cursor> {
        self.0.next.0.as_ref()
    }
}
impl Codec for CoalitionPage {
    type Decoded = Self;
    const SIZE: EncodedSize = CoalitionPageData::SIZE;
    fn encode(&self, out: &mut [u8]) -> Option<usize> {
        self.0.encode(out)
    }
    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        let (raw, used) = CoalitionPageData::decode(bytes)?;
        Some((Self::new(&raw.items, raw.next.0)?, used))
    }
}

//! Membership: who is in a coalition, and the one paged scan of that relation.
use system_protocol::TaskId;
use system_protocol::identity::CoalitionId;
use system_protocol::identity::Cursor;
use system_protocol::identity::Fail;
use system_protocol::identity::PageTarget;
use system_protocol::identity::PrincipalId;

use super::{IdentityBook, limits};

/// An id axis of the book: the slots a page scans, and when a slot joins the membership set.
pub(crate) trait Axis: Copy {
    type Page;
    const EMPTY: Self;
    fn count(book: &IdentityBook) -> usize;
    fn at(authority: TaskId, slot: u64) -> Self;
    fn slot(self) -> u64;
    fn accepts(target: &PageTarget) -> bool;
    fn joined(book: &IdentityBook, id: Self, target: &PageTarget) -> bool;
    fn page(values: &[Self], cursor: Option<Cursor>) -> Option<Self::Page>;
}

impl Axis for PrincipalId {
    type Page = system_protocol::identity::PrincipalPage;
    const EMPTY: Self = Self::EMPTY;
    fn count(book: &IdentityBook) -> usize {
        book.principals.len()
    }
    fn at(authority: TaskId, slot: u64) -> Self {
        Self { authority, slot }
    }
    fn slot(self) -> u64 {
        self.slot
    }
    fn accepts(target: &PageTarget) -> bool {
        matches!(target, PageTarget::Members(_))
    }
    fn joined(book: &IdentityBook, id: Self, target: &PageTarget) -> bool {
        matches!(target, PageTarget::Members(c) if book.memberships.contains(&(id, *c)))
    }
    fn page(values: &[Self], cursor: Option<Cursor>) -> Option<Self::Page> {
        system_protocol::identity::PrincipalPage::new(values, cursor)
    }
}

impl Axis for CoalitionId {
    type Page = system_protocol::identity::CoalitionPage;
    const EMPTY: Self = Self::EMPTY;
    fn count(book: &IdentityBook) -> usize {
        book.coalitions.len()
    }
    fn at(authority: TaskId, slot: u64) -> Self {
        Self { authority, slot }
    }
    fn slot(self) -> u64 {
        self.slot
    }
    fn accepts(target: &PageTarget) -> bool {
        matches!(target, PageTarget::Memberships(_))
    }
    fn joined(book: &IdentityBook, id: Self, target: &PageTarget) -> bool {
        matches!(target, PageTarget::Memberships(p) if book.memberships.contains(&(*p, id)))
    }
    fn page(values: &[Self], cursor: Option<Cursor>) -> Option<Self::Page> {
        system_protocol::identity::CoalitionPage::new(values, cursor)
    }
}

impl IdentityBook {
    pub fn amid(&self, p: PrincipalId, c: CoalitionId) -> Result<bool, Fail> {
        self.principal(p)?;
        self.coalition(c)?;
        Ok(self.memberships.contains(&(p, c)))
    }

    fn manage(&self, from: TaskId, member: Membership) -> Result<(), Fail> {
        let Membership {
            coalition: c,
            principal: p,
        } = member;
        let index = self.coalition(c)?;
        self.principal(p)?;
        if from == self.installer {
            return Ok(());
        }
        let sender = *self.resolve(from).ok_or(Fail::Denied)?.current.principal();
        if sender != self.coalitions[index].manager {
            return Err(Fail::NotManager);
        }
        Ok(())
    }

    pub fn admit(&mut self, from: TaskId, member: Membership) -> Result<(), Fail> {
        self.manage(from, member)?;
        let Membership {
            coalition: c,
            principal: p,
        } = member;
        if self.memberships.contains(&(p, c)) {
            return Ok(());
        }
        if self.memberships.len() >= limits::MAX_MEMBERSHIPS {
            return Err(Fail::Full);
        }
        let revision = self.revision.checked_add(1).ok_or(Fail::Full)?;
        self.memberships.try_reserve(1).map_err(|_| Fail::Full)?;
        self.memberships.push((p, c));
        self.revision = revision;
        Ok(())
    }

    pub fn expel(&mut self, from: TaskId, member: Membership) -> Result<(), Fail> {
        self.manage(from, member)?;
        let Membership {
            coalition: c,
            principal: p,
        } = member;
        let exists = self.memberships.contains(&(p, c));
        let revision = if exists {
            self.revision.checked_add(1).ok_or(Fail::Full)?
        } else {
            self.revision
        };
        self.memberships.retain(|m| *m != (p, c));
        // A revocation is transitive: the coalition leaves every binding that named it, then
        // `current` stays inside `origin` by dropping what the origin no longer holds.
        for row in &mut self.bindings {
            if *row.binding.origin.principal() == p {
                row.binding
                    .origin
                    .retain_coalitions(|selected| selected != c);
            }
            if *row.binding.current.principal() == p {
                row.binding
                    .current
                    .retain_coalitions(|selected| selected != c);
            }
            let origin = &row.binding.origin;
            row.binding
                .current
                .retain_coalitions(|selected| origin.coalitions().contains(&selected));
        }
        self.revision = revision;
        Ok(())
    }

    /// One page of the membership relation, scanned along `T`'s axis.
    pub(crate) fn page<T: Axis>(
        &self,
        target: PageTarget,
        cursor: Option<Cursor>,
    ) -> Result<T::Page, Fail> {
        if !T::accepts(&target) {
            return Err(Fail::Bad);
        }
        match &target {
            PageTarget::Members(c) => {
                self.coalition(*c)?;
            }
            PageTarget::Memberships(p) => {
                self.principal(*p)?;
            }
        }
        let after = match cursor {
            None => None,
            Some(c) if c.target != target => return Err(Fail::Bad),
            Some(c) if c.revision != self.revision => return Err(Fail::Changed),
            Some(c) => Some(c.after),
        };
        let mut ids = [T::EMPTY; limits::PAGE_ITEMS];
        let mut count = 0;
        let mut more = false;
        for slot in 0..T::count(self) as u64 {
            let id = T::at(self.authority, slot);
            if after.is_none_or(|a| id.slot() > a) && T::joined(self, id, &target) {
                if count == ids.len() {
                    more = true;
                    break;
                }
                ids[count] = id;
                count += 1;
            }
        }
        let next = more.then(|| Cursor {
            target,
            revision: self.revision,
            after: ids[count - 1].slot(),
        });
        T::page(&ids[..count], next).ok_or(Fail::Full)
    }
}

#[derive(Clone, Copy)]
pub struct Membership {
    pub coalition: CoalitionId,
    pub principal: PrincipalId,
}

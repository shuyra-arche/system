//! Bounded immutable image cache; eviction never withdraws a live Unit source.
use super::{Error, source::Shared};
use alloc::{rc::Rc, vec::Vec};
pub(super) struct Cache {
    entries: Vec<Shared>,
    closing: Vec<Shared>,
    pages: usize,
    limit: usize,
    cursor: usize,
}
impl Cache {
    pub fn new(limit: usize) -> Result<Self, Error> {
        let mut entries = Vec::new();
        let mut closing = Vec::new();
        entries
            .try_reserve_exact(limit)
            .map_err(|_| Error::Capacity)?;
        closing
            .try_reserve_exact(limit)
            .map_err(|_| Error::Capacity)?;
        Ok(Self {
            entries,
            closing,
            pages: 0,
            limit,
            cursor: 0,
        })
    }
    pub fn count(&self) -> usize {
        self.entries.len()
    }
    pub fn find(&self, contents: &[u8], prefix: usize, size: usize, flags: u64) -> Option<Shared> {
        self.entries
            .iter()
            .find(|source| source.borrow().matches(contents, prefix, size, flags))
            .cloned()
    }
    pub fn insert(&mut self, source: Shared) -> bool {
        let pages = source.borrow().pages();
        if pages > self.limit || self.closing.len() == self.limit {
            return false;
        }
        while self.pages + pages > self.limit {
            if self.closing.len() == self.limit {
                return false;
            }
            let old = self.entries.remove(0);
            self.pages -= old.borrow().pages();
            if Rc::strong_count(&old) == 1 {
                self.closing.push(old);
            }
            // A Unit holding a second Rc remains responsible for final cleanup.
        }
        self.pages += pages;
        self.entries.push(source);
        true
    }
    /// One retirement attempt per round, rotating past a contended source.
    pub fn sweep(&mut self) -> Result<bool, Error> {
        if self.closing.is_empty() {
            return Ok(true);
        }
        let index = self.cursor % self.closing.len();
        let result = self.closing[index].borrow_mut().finish();
        match result {
            Ok(()) => {
                self.closing.swap_remove(index);
            }
            Err(error) if error.busy() => (),
            Err(error) => return Err(error),
        }
        self.cursor = self.cursor.wrapping_add(1);
        Ok(self.closing.is_empty())
    }
    pub fn clear(&mut self) -> Result<bool, Error> {
        self.sweep()?;
        while !self.entries.is_empty() && self.closing.len() < self.limit {
            let old = self.entries.pop().unwrap();
            self.pages -= old.borrow().pages();
            if Rc::strong_count(&old) == 1 {
                self.closing.push(old);
            }
        }
        Ok(self.entries.is_empty() && self.sweep()?)
    }
}

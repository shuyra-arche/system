use alloc::vec::Vec;
use kernel_api::model::identity::PendId;
use system_protocol::operator::{EntryTarget, GrantId, GrantSpec, GrantState, Ops};
use system_protocol::{Path, TaskId};

const LIMIT: usize = 256;

pub struct GrantSubmission {
    pub via: PendId,
    pub caller: TaskId,
    pub spec: GrantSpec,
}

struct Record {
    identity: Identity,
    grant: Grant,
    cleanup: Cleanup,
}

struct Grant {
    spec: GrantSpec,
    state: GrantState,
}

#[derive(Clone, Copy)]
struct Identity {
    key: GrantKey,
    issuer: TaskId,
    via: PendId,
    parent: Option<GrantKey>,
}

#[derive(Default)]
struct Cleanup {
    entrance: Option<PendId>,
    remote: Option<system_protocol::PendId>,
    attached: bool,
    stage: CleanupStage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CleanupStage {
    #[default]
    Registered,
    Detached,
    Returned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupProgress {
    pub attached: bool,
    pub stage: CleanupStage,
}

/// Completion receipts from the service owner, after each native operation
/// succeeds. Returning an entrance retires its whole Pend branch; Busy must
/// keep the owner at Detached for the next attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupStep {
    Detach,
    Return,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GrantKey {
    Root,
    Issued(core::num::NonZeroU64),
}

#[derive(Clone, Copy)]
pub struct GrantRef {
    pub via: PendId,
    pub caller: TaskId,
    pub id: GrantId,
}

pub struct Retirement {
    pub key: GrantKey,
    pub reference: GrantRef,
    pub entrance: Option<PendId>,
    depth: usize,
}

pub struct RootGrant {
    pub system: TaskId,
    pub operator: TaskId,
    pub entrance: PendId,
}

#[derive(Clone, Copy)]
struct Watermark {
    via: PendId,
    caller: TaskId,
    high_water: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthorityError {
    Denied,
    Conflict,
    Stale,
    Full,
    Unknown,
}

/// Bounded authorization history for one native Operator entrance.
pub struct Authority {
    root: TaskId,
    operator: TaskId,
    entrance: Option<PendId>,
    records: Vec<Record>,
    watermarks: Vec<Watermark>,
    next_key: u64,
}

impl Authority {
    pub fn new(root: TaskId, operator: TaskId) -> Self {
        Self {
            root,
            operator,
            entrance: None,
            records: Vec::new(),
            watermarks: Vec::new(),
            next_key: 1,
        }
    }

    /// Root is a trusted bootstrap admission, not a wire GrantId sentinel.
    pub fn bootstrap_root(&mut self, root: RootGrant) -> Result<(), AuthorityError> {
        if root.system != self.root
            || root.operator != self.operator
            || self.root == TaskId::NONE
            || self.operator == TaskId::NONE
        {
            return Err(AuthorityError::Denied);
        }
        if let Some(old) = self.entrance {
            return if old == root.entrance {
                Ok(())
            } else {
                Err(AuthorityError::Conflict)
            };
        }
        if !self.records.is_empty() {
            return Err(AuthorityError::Conflict);
        }
        self.entrance = Some(root.entrance);
        Ok(())
    }

    pub fn grant(
        &mut self,
        request: GrantSubmission,
        relative_anchor: &Path,
    ) -> Result<(GrantState, GrantKey), AuthorityError> {
        let GrantSubmission { caller, via, spec } = request;
        let (parent_key, parent_spec) = self.grant_at(via, caller).ok_or(AuthorityError::Denied)?;
        if spec.parent.0 != parent_spec.map(|parent| parent.id) {
            return Err(AuthorityError::Denied);
        }
        if let Some(old) = self.records.iter().find(|r| {
            r.identity.via == via && r.identity.issuer == caller && r.grant.spec.id == spec.id
        }) {
            return if old.grant.spec == spec && old.identity.parent == Some(parent_key) {
                Ok((old.grant.state, old.identity.key))
            } else {
                Err(AuthorityError::Conflict)
            };
        }
        let watermark = self
            .watermarks
            .iter()
            .position(|w| w.via == via && w.caller == caller);
        if watermark.is_some_and(|at| spec.id.get() <= self.watermarks[at].high_water) {
            return Err(AuthorityError::Stale);
        }
        if self.records.len() >= LIMIT {
            return Err(AuthorityError::Full);
        }
        if spec.holder == TaskId::NONE
            || spec.holder == self.root
            || spec.anchor.operator != self.operator
            || !spec.anchor.valid()
            || matches!(spec.anchor.target, EntryTarget::Root)
            || spec.source.0 == Some(TaskId::NONE)
        {
            return Err(AuthorityError::Denied);
        }
        if let Some(parent) = parent_spec
            && (!parent.ops.contains(Ops::GRANT)
                || parent
                    .permit
                    .0
                    .as_ref()
                    .is_some_and(|permit| spec.permit.0.as_ref() != Some(permit))
                || parent
                    .source
                    .0
                    .is_some_and(|source| spec.source.0 != Some(source))
                || parent
                    .source_mark
                    .0
                    .is_some_and(|mark| spec.source_mark.0 != Some(mark))
                || !parent.ops.contains(spec.ops)
                || !grant_scope_narrows(parent, &spec, relative_anchor))
        {
            return Err(AuthorityError::Denied);
        }
        self.records
            .try_reserve(1)
            .map_err(|_| AuthorityError::Full)?;
        let next_key = self.next_key.checked_add(1).ok_or(AuthorityError::Full)?;
        let key = GrantKey::Issued(
            core::num::NonZeroU64::new(self.next_key).ok_or(AuthorityError::Full)?,
        );
        if let Some(at) = watermark {
            self.watermarks[at].high_water = spec.id.get();
        } else {
            if self.watermarks.len() >= LIMIT {
                return Err(AuthorityError::Full);
            }
            self.watermarks
                .try_reserve(1)
                .map_err(|_| AuthorityError::Full)?;
            self.watermarks.push(Watermark {
                via,
                caller,
                high_water: spec.id.get(),
            });
        }
        self.next_key = next_key;
        self.records.push(Record {
            identity: Identity {
                key,
                issuer: caller,
                via,
                parent: Some(parent_key),
            },
            grant: Grant {
                spec,
                state: GrantState::Pending,
            },
            cleanup: Cleanup::default(),
        });
        Ok((GrantState::Pending, key))
    }

    pub fn delivered(
        &mut self,
        key: GrantRef,
        remote: system_protocol::PendId,
    ) -> Result<(), AuthorityError> {
        if remote == system_protocol::PendId::NONE {
            return Err(AuthorityError::Denied);
        }
        let native_key = self
            .records
            .iter()
            .find(|r| {
                r.identity.via == key.via
                    && r.identity.issuer == key.caller
                    && r.grant.spec.id == key.id
            })
            .map(|r| r.identity.key)
            .ok_or(AuthorityError::Unknown)?;
        if !self.ancestors_live(native_key) {
            return Err(AuthorityError::Denied);
        }
        let r = self
            .records
            .iter_mut()
            .find(|r| {
                r.identity.via == key.via
                    && r.grant.spec.id == key.id
                    && r.identity.issuer == key.caller
            })
            .ok_or(AuthorityError::Unknown)?;
        if r.grant.state == GrantState::Granted {
            return if r.cleanup.remote == Some(remote) {
                Ok(())
            } else {
                Err(AuthorityError::Conflict)
            };
        }
        if r.grant.state != GrantState::Pending || !r.cleanup.attached {
            return Err(AuthorityError::Denied);
        }
        r.cleanup.remote = Some(remote);
        r.grant.state = GrantState::Granted;
        Ok(())
    }

    pub fn remote(&self, key: GrantKey) -> Option<system_protocol::PendId> {
        if !self.live(key) {
            return None;
        }
        self.records
            .iter()
            .find(|record| record.identity.key == key && record.grant.state == GrantState::Granted)
            .and_then(|record| record.cleanup.remote)
    }

    pub fn inspect(&self, key: GrantRef) -> Result<GrantState, AuthorityError> {
        if let Some(r) = self.records.iter().find(|r| {
            r.identity.via == key.via
                && r.grant.spec.id == key.id
                && r.identity.issuer == key.caller
        }) {
            return Ok(r.grant.state);
        }
        let high_water = self
            .watermarks
            .iter()
            .find(|w| w.via == key.via && w.caller == key.caller)
            .map_or(0, |w| w.high_water);
        if key.id.get() <= high_water {
            Ok(GrantState::Closed)
        } else {
            Ok(GrantState::Unseen)
        }
    }

    pub fn revoke(&mut self, key: GrantRef) -> Result<GrantState, AuthorityError> {
        if let Some(r) = self.records.iter_mut().find(|r| {
            r.identity.via == key.via
                && r.grant.spec.id == key.id
                && r.identity.issuer == key.caller
        }) {
            if r.grant.state == GrantState::Closed {
                return Ok(GrantState::Closed);
            }
            r.grant.state = GrantState::Revoking;
            return Ok(GrantState::Revoking);
        }
        let at = self
            .watermarks
            .iter()
            .position(|w| w.via == key.via && w.caller == key.caller);
        if at.is_some_and(|at| key.id.get() <= self.watermarks[at].high_water) {
            return Ok(GrantState::Closed);
        }
        if let Some(at) = at {
            self.watermarks[at].high_water = key.id.get();
        } else {
            if self.watermarks.len() >= LIMIT {
                return Err(AuthorityError::Full);
            }
            self.watermarks
                .try_reserve(1)
                .map_err(|_| AuthorityError::Full)?;
            self.watermarks.push(Watermark {
                via: key.via,
                caller: key.caller,
                high_water: key.id.get(),
            });
        }
        Ok(GrantState::Closed)
    }

    pub fn revoke_plan(&mut self, key: GrantRef) -> Result<Vec<Retirement>, AuthorityError> {
        let Some(root) = self
            .records
            .iter()
            .find(|r| {
                r.identity.via == key.via
                    && r.identity.issuer == key.caller
                    && r.grant.spec.id == key.id
            })
            .map(|r| r.identity.key)
        else {
            return Ok(Vec::new());
        };
        let mut plan = Vec::new();
        plan.try_reserve(self.records.len())
            .map_err(|_| AuthorityError::Full)?;
        for record in &self.records {
            let mut parent = record.identity.parent;
            let mut descendant = record.identity.key == root;
            let mut remaining = self.records.len();
            let mut depth = 0;
            while !descendant && parent.is_some() && remaining > 0 {
                remaining -= 1;
                depth += 1;
                let current = parent.unwrap();
                if current == root {
                    descendant = true;
                    break;
                }
                parent = self
                    .records
                    .iter()
                    .find(|candidate| candidate.identity.key == current)
                    .and_then(|candidate| candidate.identity.parent);
            }
            if descendant && record.grant.state != GrantState::Closed {
                plan.push(Retirement {
                    key: record.identity.key,
                    reference: GrantRef {
                        via: record.identity.via,
                        caller: record.identity.issuer,
                        id: record.grant.spec.id,
                    },
                    entrance: record.cleanup.entrance,
                    depth,
                });
            }
        }
        for record in &mut self.records {
            if plan.iter().any(|item| item.key == record.identity.key) {
                record.grant.state = GrantState::Revoking;
            }
        }
        // Closing another branch can swap storage slots. Physical record
        // order is not ancestry order; deepest descendants retire first.
        plan.sort_unstable_by_key(|item| core::cmp::Reverse(item.depth));
        Ok(plan)
    }

    pub fn closed(&mut self, key: GrantRef) -> Result<(), AuthorityError> {
        let at = self
            .records
            .iter()
            .position(|r| {
                r.identity.via == key.via
                    && r.grant.spec.id == key.id
                    && r.identity.issuer == key.caller
            })
            .ok_or(AuthorityError::Unknown)?;
        let key = self.records[at].identity.key;
        let entrance = self.records[at].cleanup.entrance;
        if self.records[at].grant.state != GrantState::Revoking
            && self.records[at].grant.state != GrantState::Closed
        {
            return Err(AuthorityError::Denied);
        }
        if entrance.is_some() && self.records[at].cleanup.stage != CleanupStage::Returned {
            return Err(AuthorityError::Denied);
        }
        if self
            .records
            .iter()
            .any(|r| r.identity.parent == Some(key) && r.grant.state != GrantState::Closed)
        {
            return Err(AuthorityError::Denied);
        }
        self.records.swap_remove(at);
        if let Some(entrance) = entrance {
            self.watermarks
                .retain(|watermark| watermark.via != entrance);
        }
        Ok(())
    }

    pub fn set_entrance(&mut self, key: GrantRef, entrance: PendId) -> Result<(), AuthorityError> {
        if self.entrance == Some(entrance)
            || self.records.iter().any(|r| {
                r.cleanup.entrance == Some(entrance)
                    && !(r.identity.via == key.via
                        && r.grant.spec.id == key.id
                        && r.identity.issuer == key.caller)
            })
        {
            return Err(AuthorityError::Conflict);
        }
        let r = self
            .records
            .iter_mut()
            .find(|r| {
                r.identity.via == key.via
                    && r.grant.spec.id == key.id
                    && r.identity.issuer == key.caller
            })
            .ok_or(AuthorityError::Unknown)?;
        if r.grant.state != GrantState::Pending {
            return Err(AuthorityError::Denied);
        }
        if r.cleanup.entrance.is_some_and(|old| old != entrance) {
            return Err(AuthorityError::Conflict);
        }
        r.cleanup.entrance = Some(entrance);
        Ok(())
    }

    pub fn grant_at(
        &self,
        entrance: PendId,
        caller: TaskId,
    ) -> Option<(GrantKey, Option<&GrantSpec>)> {
        if self.entrance == Some(entrance) && caller == self.root {
            return Some((GrantKey::Root, None));
        }
        self.records
            .iter()
            .find(|r| {
                r.cleanup.entrance == Some(entrance)
                    && r.grant.spec.holder == caller
                    && r.grant.state == GrantState::Granted
            })
            .filter(|r| self.ancestors_live(r.identity.key))
            .map(|r| (r.identity.key, Some(&r.grant.spec)))
    }

    pub fn entrance(&self, key: GrantRef) -> Option<PendId> {
        self.records
            .iter()
            .find(|r| {
                r.identity.via == key.via
                    && r.identity.issuer == key.caller
                    && r.grant.spec.id == key.id
            })
            .and_then(|r| r.cleanup.entrance)
    }

    pub fn root_entrance(&self) -> Option<PendId> {
        self.entrance
    }

    pub fn by_entrance(&self, entrance: PendId) -> Option<GrantKey> {
        if self.entrance == Some(entrance) {
            return Some(GrantKey::Root);
        }
        self.records
            .iter()
            .find(|r| r.cleanup.entrance == Some(entrance))
            .map(|r| r.identity.key)
    }

    pub fn live(&self, key: GrantKey) -> bool {
        if key == GrantKey::Root {
            return self.entrance.is_some();
        }
        self.records
            .iter()
            .any(|r| r.identity.key == key && r.grant.state == GrantState::Granted)
            && self.ancestors_live(key)
    }

    pub fn by_id(&self, key: GrantKey, caller: TaskId) -> Option<(GrantId, &GrantSpec)> {
        let record = self.records.iter().find(|r| {
            r.identity.key == key
                && r.grant.state == GrantState::Granted
                && r.grant.spec.holder == caller
        })?;
        self.ancestors_live(key)
            .then_some((record.grant.spec.id, &record.grant.spec))
    }

    pub fn cleanup_progress(&self, key: GrantKey) -> Option<CleanupProgress> {
        let r = self
            .records
            .iter()
            .find(|record| record.identity.key == key)?;
        Some(CleanupProgress {
            attached: r.cleanup.attached,
            stage: r.cleanup.stage,
        })
    }

    pub fn attached(&mut self, key: GrantKey) -> Result<(), AuthorityError> {
        let r = self
            .records
            .iter_mut()
            .find(|record| record.identity.key == key)
            .ok_or(AuthorityError::Unknown)?;
        if r.grant.state != GrantState::Pending || r.cleanup.entrance.is_none() {
            return Err(AuthorityError::Denied);
        }
        r.cleanup.attached = true;
        Ok(())
    }

    pub fn cleanup_step(&mut self, key: GrantKey, step: CleanupStep) -> Result<(), AuthorityError> {
        let r = self
            .records
            .iter_mut()
            .find(|record| record.identity.key == key)
            .ok_or(AuthorityError::Unknown)?;
        if r.grant.state != GrantState::Revoking {
            return Err(AuthorityError::Denied);
        }
        match (r.cleanup.stage, step) {
            (CleanupStage::Registered, CleanupStep::Detach) => {
                r.cleanup.stage = CleanupStage::Detached
            }
            (CleanupStage::Detached, CleanupStep::Return) => {
                r.cleanup.stage = CleanupStage::Returned
            }
            (CleanupStage::Detached | CleanupStage::Returned, CleanupStep::Detach)
            | (CleanupStage::Returned, CleanupStep::Return) => {}
            _ => return Err(AuthorityError::Denied),
        }
        Ok(())
    }

    fn ancestors_live(&self, key: GrantKey) -> bool {
        let mut next = self
            .records
            .iter()
            .find(|r| r.identity.key == key)
            .and_then(|r| r.identity.parent);
        let mut remaining = self.records.len();
        while let Some(parent) = next {
            if parent == GrantKey::Root {
                return self.entrance.is_some();
            }
            if remaining == 0 {
                return false;
            }
            remaining -= 1;
            let Some(record) = self
                .records
                .iter()
                .find(|r| r.identity.key == parent && r.grant.state == GrantState::Granted)
            else {
                return false;
            };
            next = record.identity.parent;
        }
        true
    }
}

fn grant_scope_narrows(parent: &GrantSpec, child: &GrantSpec, relative_anchor: &Path) -> bool {
    if relative_anchor.as_str().is_empty() {
        parent.anchor == child.anchor && parent.scope.narrows(&child.scope)
    } else {
        parent.scope.narrows_at(relative_anchor, &child.scope)
    }
}

#[cfg(test)]
#[path = "../../../../tests/operator-policy/authority.rs"]
mod tests;

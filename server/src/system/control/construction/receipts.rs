//! Caller-scoped construction receipt state; no native effects or wire authority.
use alloc::vec::Vec;
use kernel_api::model::identity::TaskId;
use system_protocol::loader::{self as api, Fail};

/// One caller's retained Build receipt. The requested shape, selected instance,
/// recipient-local grants, claim bit and deadline form one construction record.
pub(crate) struct Creation {
    caller: TaskId,
    request: api::Build,
    instance: system_protocol::control::instance::Sequence,
    resolve: Option<system_protocol::PendId>,
    owner_resolve: Option<system_protocol::PendId>,
    claimed: bool,
    expires: u64,
}

impl Creation {
    pub(crate) fn new(
        caller: TaskId,
        request: api::Build,
        instance: system_protocol::control::instance::Sequence,
        expires: u64,
    ) -> Self {
        Self {
            caller,
            request,
            instance,
            resolve: None,
            owner_resolve: None,
            claimed: false,
            expires,
        }
    }
}

/// Owns Build replay/conflict decisions and bounded receipt state. Instance
/// lifecycle and native effects remain with Control and Instance.
pub(crate) struct Ledger {
    records: Vec<Creation>,
    capacity: usize,
}

impl Ledger {
    pub(crate) fn new(capacity: usize) -> Result<Self, &'static str> {
        let mut records = Vec::new();
        records
            .try_reserve_exact(capacity)
            .map_err(|_| "construction capacity")?;
        Ok(Self { records, capacity })
    }

    /// Returns the existing receipt for an identical replay, rejects reuse of
    /// the caller's ID for another request, or confirms a fresh slot is free.
    pub(crate) fn replay(
        &self,
        caller: TaskId,
        request: &api::Build,
    ) -> Result<Option<usize>, Fail> {
        if let Some(at) = self
            .records
            .iter()
            .position(|record| record.caller == caller && record.request.id == request.id)
        {
            return if self.records[at].request == *request {
                Ok(Some(at))
            } else {
                Err(Fail::Denied)
            };
        }
        if self.records.len() == self.capacity {
            return Err(Fail::Full);
        }
        Ok(None)
    }

    pub(crate) fn insert(&mut self, record: Creation) {
        assert!(matches!(
            self.replay(record.caller, &record.request),
            Ok(None)
        ));
        assert!(self.records.len() < self.capacity);
        self.records.push(record);
    }

    /// Unclaimed records expire by deadline or owner exit. Control applies the
    /// resulting stop requests through the sole Instance owner.
    pub(crate) fn expired_instances<'a>(
        &'a self,
        now: u64,
        mut owner_exited: impl FnMut(TaskId) -> bool + 'a,
    ) -> impl Iterator<Item = system_protocol::control::instance::Sequence> + 'a {
        self.records.iter().filter_map(move |record| {
            (!record.claimed && (now >= record.expires || owner_exited(record.caller)))
                .then_some(record.instance)
        })
    }

    pub(crate) fn find(&self, caller: TaskId, id: api::CreateId) -> Option<usize> {
        self.records
            .iter()
            .position(|record| record.caller == caller && record.request.id == id)
    }

    pub(crate) fn find_claim(
        &self,
        caller: TaskId,
        task: system_protocol::TaskId,
        mut instance: impl FnMut(
            system_protocol::control::instance::Sequence,
        ) -> Option<(TaskId, system_protocol::TaskId)>,
    ) -> Option<usize> {
        self.records.iter().position(|record| {
            instance(record.instance)
                .is_some_and(|(owner, observed)| owner == caller && observed == task)
        })
    }

    pub(crate) fn instance(&self, at: usize) -> system_protocol::control::instance::Sequence {
        self.records[at].instance
    }

    pub(crate) fn resolve_with<E>(
        &mut self,
        at: usize,
        create: impl FnOnce() -> Result<system_protocol::PendId, E>,
    ) -> Result<system_protocol::PendId, E> {
        if let Some(resolve) = self.records[at].resolve {
            return Ok(resolve);
        }
        let resolve = create()?;
        self.records[at].resolve = Some(resolve);
        Ok(resolve)
    }

    pub(crate) fn claim_with<E>(
        &mut self,
        at: usize,
        create: impl FnOnce() -> Result<system_protocol::PendId, E>,
    ) -> Result<system_protocol::PendId, E> {
        let resolve = match self.records[at].owner_resolve {
            Some(resolve) => resolve,
            None => {
                let resolve = create()?;
                self.records[at].owner_resolve = Some(resolve);
                resolve
            }
        };
        self.records[at].claimed = true;
        Ok(resolve)
    }
}

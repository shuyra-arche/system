//! The Operator owns Instance-to-Pane identity, root lifetime and stable-name
//! revisions. Grant keys refer to Authority's records; no grant state is copied.
use crate::system::operator::authority::GrantKey;
use alloc::vec::Vec;
use system_protocol::{
    Mark, Name,
    control::instance::InstanceId,
    operator::{Fail, unit as api},
    value::Optional,
};

struct Transition {
    request: api::Advance,
    receipt: api::Root,
}
pub(super) struct Record {
    root: api::Root,
    grants: Vec<GrantKey>,
    transitions: Vec<Transition>,
}
impl Record {
    pub fn root(&self) -> &api::Root {
        &self.root
    }
    pub fn grants(&self) -> &[GrantKey] {
        &self.grants
    }
}
struct Publication {
    request: api::Associate,
    receipt: api::Association,
}
#[derive(Default)]
pub(super) struct Registry {
    records: Vec<Record>,
    names: Vec<api::Association>,
    publications: Vec<Publication>,
}
impl Registry {
    pub fn records(&self) -> &[Record] {
        &self.records
    }
    pub fn prepare_grant(&mut self, at: usize) -> Result<(), Fail> {
        let record = &mut self.records[at];
        if !matches!(
            record.root.phase,
            api::Phase::Preparing | api::Phase::Active
        ) {
            return Err(Fail::Denied);
        }
        record.grants.try_reserve(1).map_err(|_| Fail::Full)
    }
    pub fn link_grant(&mut self, at: usize, key: GrantKey) {
        let record = &mut self.records[at];
        if !record.grants.contains(&key) {
            record.grants.push(key);
        }
    }
    pub fn get(&self, id: InstanceId) -> Result<&api::Root, Fail> {
        self.records
            .iter()
            .find(|row| row.root.registration.instance == id)
            .map(|row| &row.root)
            .ok_or(Fail::Unknown)
    }
    pub fn registered(&self, request: &api::Registration) -> Option<Result<api::Root, Fail>> {
        self.records
            .iter()
            .find(|row| row.root.registration.instance == request.instance)
            .map(|row| {
                if row.root.registration == *request {
                    // Registration is always a receipt for the Preparing commit.
                    Ok(api::Root {
                        registration: request.clone(),
                        revision: 0,
                        phase: api::Phase::Preparing,
                    })
                } else {
                    Err(Fail::Conflict)
                }
            })
    }
    pub fn register(&mut self, request: api::Registration) -> Result<api::Root, Fail> {
        if let Some(result) = self.registered(&request) {
            return result;
        }
        if self.records.len() == 256 {
            return Err(Fail::Full);
        }
        if self.records.iter().any(|row| {
            row.root.registration.pane.entry == request.pane.entry
                || row.root.registration.task == request.task
        }) {
            return Err(Fail::Conflict);
        }
        self.records.try_reserve(1).map_err(|_| Fail::Full)?;
        let mut transitions = Vec::new();
        transitions.try_reserve_exact(4).map_err(|_| Fail::Full)?;
        let root = api::Root {
            registration: request,
            revision: 0,
            phase: api::Phase::Preparing,
        };
        self.records.push(Record {
            root: root.clone(),
            grants: Vec::new(),
            transitions,
        });
        Ok(root)
    }
    pub fn replay(&self, request: &api::Advance) -> Option<Result<api::Root, Fail>> {
        self.records
            .iter()
            .find(|row| row.root.registration.instance == request.instance)
            .and_then(|row| {
                row.transitions
                    .iter()
                    .find(|old| old.request.expected == request.expected)
            })
            .map(|old| {
                if old.request == *request {
                    Ok(old.receipt.clone())
                } else {
                    Err(Fail::Conflict)
                }
            })
    }
    /// Called after the serialized Operator has checked native settlement for
    /// Retired. Closing removes future discovery, retaining old consumer grants.
    pub fn advance(&mut self, request: api::Advance) -> Result<api::Root, Fail> {
        use api::Phase as P;
        if let Some(result) = self.replay(&request) {
            return result;
        }
        let at = self
            .records
            .iter()
            .position(|row| row.root.registration.instance == request.instance)
            .ok_or(Fail::Unknown)?;
        let root = &self.records[at].root;
        if root.revision != request.expected {
            return Err(Fail::Conflict);
        }
        if !matches!(
            (root.phase, request.phase),
            (P::Preparing, P::Active | P::Closing)
                | (P::Active, P::Closing)
                | (P::Closing, P::Retiring)
                | (P::Retiring, P::Retired)
        ) {
            return Err(Fail::Bad);
        }
        let revision = root.revision.checked_add(1).ok_or(Fail::Full)?;
        if request.phase == P::Closing {
            for name in &self.names {
                if name.instance.0 == Some(request.instance) {
                    name.revision.checked_add(1).ok_or(Fail::Full)?;
                }
            }
            for name in &mut self.names {
                if name.instance.0 == Some(request.instance) {
                    name.revision += 1;
                    name.instance = Optional(None);
                    name.pane = Optional(None);
                }
            }
        }
        let row = &mut self.records[at];
        row.root.revision = revision;
        row.root.phase = request.phase;
        let receipt = row.root.clone();
        row.transitions.push(Transition {
            request,
            receipt: receipt.clone(),
        });
        Ok(receipt)
    }
    pub fn associate(&mut self, request: api::Associate) -> Result<api::Association, Fail> {
        if let Some(old) = self
            .publications
            .iter()
            .find(|old| old.request.id == request.id)
        {
            return if old.request == request {
                Ok(old.receipt.clone())
            } else {
                Err(Fail::Conflict)
            };
        }
        if request.mark.get() != kernel_api::model::identity::Mark::of(request.name.as_str()).get()
        {
            return Err(Fail::Bad);
        }
        let at = self.names.iter().position(|name| name.mark == request.mark);
        if let Some(at) = at
            && self.names[at].name != request.name
        {
            return Err(Fail::Conflict);
        }
        let revision = at.map_or(0, |at| self.names[at].revision);
        if revision != request.expected {
            return Err(Fail::Conflict);
        }
        let pane = if let Some(id) = request.instance.0 {
            let root = self.get(id)?;
            if root.phase != api::Phase::Active {
                return Err(Fail::Denied);
            }
            Some(root.registration.pane.entry.clone())
        } else {
            None
        };
        let owner = match request.instance.0 {
            Some(id) => self.get(id)?.registration.owner,
            None => at.map(|at| self.names[at].owner).ok_or(Fail::Unknown)?,
        };
        if at.is_some_and(|at| self.names[at].owner != owner) {
            return Err(Fail::Denied);
        }
        if self.publications.len() == 256 || at.is_none() && self.names.len() == 256 {
            return Err(Fail::Full);
        }
        self.publications.try_reserve(1).map_err(|_| Fail::Full)?;
        if at.is_none() {
            self.names.try_reserve(1).map_err(|_| Fail::Full)?;
        }
        let receipt = api::Association {
            mark: request.mark,
            name: request.name.clone(),
            revision: revision.checked_add(1).ok_or(Fail::Full)?,
            owner,
            instance: request.instance.clone(),
            pane: Optional(pane),
        };
        match at {
            Some(at) => self.names[at] = receipt.clone(),
            None => self.names.push(receipt.clone()),
        }
        self.publications.push(Publication {
            request,
            receipt: receipt.clone(),
        });
        Ok(receipt)
    }
    pub fn resolve(&self, mark: Mark, name: &Name) -> Result<api::Association, Fail> {
        let row = self
            .names
            .iter()
            .find(|row| row.mark == mark)
            .ok_or(Fail::Unknown)?;
        if row.name != *name {
            return Err(Fail::Conflict);
        }
        Ok(row.clone())
    }
}

//! Caller/ID correlation only. Native roots and completion belong to Instance.
use alloc::{sync::Arc, vec::Vec};
use kernel_api::model::identity::TaskId;
use system_protocol::supply::{Fail, Install, InstallId, MAX_INSTALLATIONS};
#[derive(Clone)]
pub(crate) enum Intent {
    Accepted(Arc<Install>),
    Cancelled(InstallId),
}
impl Intent {
    pub fn id(&self) -> InstallId {
        match self {
            Self::Accepted(request) => request.id,
            Self::Cancelled(id) => *id,
        }
    }
}
struct Record {
    caller: TaskId,
    intent: Intent,
}
#[derive(Default)]
pub(crate) struct Journal {
    records: Vec<Record>,
}
impl Journal {
    pub fn find(&self, caller: TaskId, id: InstallId) -> Option<Intent> {
        self.records
            .iter()
            .find(|row| row.caller == caller && row.intent.id() == id)
            .map(|row| row.intent.clone())
    }
    pub fn replay(&self, caller: TaskId, request: &Install) -> Result<Option<Intent>, Fail> {
        let old = self.find(caller, request.id);
        if let Some(Intent::Accepted(old)) = &old
            && **old != *request
        {
            return Err(Fail::Conflict);
        }
        Ok(old)
    }
    pub fn reserve(&mut self) -> Result<(), Fail> {
        if self.records.len() == MAX_INSTALLATIONS {
            return Err(Fail::Full);
        }
        self.records.try_reserve(1).map_err(|_| Fail::Full)
    }
    pub fn insert(&mut self, caller: TaskId, request: Arc<Install>) {
        assert!(self.find(caller, request.id).is_none());
        assert!(
            self.records.len() < MAX_INSTALLATIONS && self.records.len() < self.records.capacity()
        );
        self.records.push(Record {
            caller,
            intent: Intent::Accepted(request),
        });
    }
    pub fn cancel(&mut self, caller: TaskId, id: InstallId) -> Result<Intent, Fail> {
        if let Some(old) = self.find(caller, id) {
            return Ok(old);
        }
        self.reserve()?;
        let intent = Intent::Cancelled(id);
        self.records.push(Record {
            caller,
            intent: intent.clone(),
        });
        Ok(intent)
    }
}

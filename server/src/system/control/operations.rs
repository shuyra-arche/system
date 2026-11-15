//! One bounded receipt journal. It owns accepted intent and completion, never
//! Instance lifecycle or native resources. The serial Control owner commits
//! effects after reserving a journal slot, then advances from Instance evidence.
use alloc::{boxed::Box, vec::Vec};
use kernel_api::model::identity::TaskId;
use system_protocol::{
    control::instance::{Fail, InstanceId, OperationId, Receipt, State},
    value::Optional,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Intent {
    Create(Box<system_protocol::control::instance::Create>),
    CreateFor(Box<system_protocol::control::instance::CreateFor>),
    Start(InstanceId),
    Stop(InstanceId),
    Retire(InstanceId),
    Publish(system_protocol::control::instance::Publication),
}

enum Completion {
    Pending,
    Cancelling,
    Finished(State, Option<Fail>),
}
struct Record {
    caller: TaskId,
    id: OperationId,
    intent: Intent,
    target: Option<InstanceId>,
    completion: Completion,
}

/// Evidence supplied by the sole Instance owner, at the current serial turn.
pub(super) struct Evidence {
    pub constructed: bool,
    pub started: bool,
    pub stopped: bool,
    pub retired: bool,
    pub failure: Option<Fail>,
}

pub(super) struct Journal {
    records: Vec<Record>,
    capacity: usize,
}
impl Journal {
    pub fn new(capacity: usize) -> Result<Self, &'static str> {
        let mut records = Vec::new();
        records
            .try_reserve_exact(capacity)
            .map_err(|_| "operation capacity")?;
        Ok(Self { records, capacity })
    }
    /// A repeated intent returns its original index without committing again.
    /// Full journals retain every receipt; no accepted ID is silently reused.
    pub fn admit(
        &mut self,
        caller: TaskId,
        id: OperationId,
        intent: Intent,
    ) -> Result<(usize, bool), Fail> {
        if let Some(at) = self.find(caller, id) {
            return if self.records[at].intent == intent {
                Ok((at, false))
            } else {
                Err(Fail::Conflict)
            };
        }
        if self.records.len() == self.capacity {
            return Err(Fail::Full);
        }
        let at = self.records.len();
        self.records.push(Record {
            caller,
            id,
            intent,
            target: None,
            completion: Completion::Pending,
        });
        Ok((at, true))
    }
    pub fn find(&self, caller: TaskId, id: OperationId) -> Option<usize> {
        self.records
            .iter()
            .position(|record| record.caller == caller && record.id == id)
    }
    pub fn receipt(&self, at: usize) -> Receipt {
        let record = &self.records[at];
        let (state, failure) = match record.completion {
            Completion::Pending | Completion::Cancelling => (State::Pending, None),
            Completion::Finished(state, failure) => (state, failure),
        };
        Receipt {
            id: record.id,
            instance: Optional(record.target),
            state,
            failure: Optional(failure.map(weave::Coded)),
        }
    }
    pub fn attach(&mut self, at: usize, instance: InstanceId) {
        assert!(self.records[at].target.is_none());
        self.records[at].target = Some(instance);
    }
    pub fn reject(&mut self, at: usize, failure: Fail) {
        self.records[at].completion = Completion::Finished(State::Failed, Some(failure));
    }
    pub fn complete(&mut self, at: usize) {
        assert!(matches!(self.records[at].completion, Completion::Pending));
        self.records[at].completion = Completion::Finished(State::Completed, None);
    }
    /// Terminal receipts cannot be undone. Pending cancellation retains the
    /// operation until the Instance owner proves all native cleanup complete.
    pub fn cancel(&mut self, at: usize) -> Option<InstanceId> {
        let record = &mut self.records[at];
        match record.completion {
            Completion::Pending | Completion::Cancelling => {
                record.completion = Completion::Cancelling;
                record.target
            }
            Completion::Finished(..) => None,
        }
    }
    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn target(&self, at: usize) -> Option<InstanceId> {
        self.records[at].target
    }
    pub fn advance(&mut self, at: usize, evidence: Evidence) {
        let record = &mut self.records[at];
        match record.completion {
            Completion::Finished(..) => return,
            Completion::Cancelling => {
                if evidence.retired {
                    record.completion = Completion::Finished(State::Cancelled, evidence.failure);
                }
                return;
            }
            Completion::Pending => (),
        }
        let complete = match record.intent {
            Intent::Create(_) | Intent::CreateFor(_) => evidence.constructed,
            Intent::Start(_) => evidence.started,
            Intent::Stop(_) => evidence.stopped,
            Intent::Retire(_) => evidence.retired,
            Intent::Publish(_) => false,
        };
        if complete {
            record.completion = Completion::Finished(State::Completed, None);
        } else if evidence.retired {
            record.completion = Completion::Finished(
                State::Failed,
                Some(evidence.failure.unwrap_or(Fail::Interrupted)),
            );
        }
    }
}

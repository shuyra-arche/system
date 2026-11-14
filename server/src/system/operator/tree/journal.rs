//! Bounded mutation receipts. A timeout or failed reply never makes a committed
//! mutation executable again. Replay identity is native ingress plus sender.
use alloc::vec::Vec;
use kernel_api::model::identity::PendId;
use system_protocol::{
    TaskId,
    operator::{self as api, Binding, Fail, MutationStatus, OperationId, OperationState},
    value::Optional,
};
use weave::Coded;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Intent {
    Part(api::Part),
    Publish {
        replace: bool,
        change: api::Mutation,
        name: system_protocol::Name,
        resource_id: u64,
        rights: system_protocol::value::Rights,
        permit: api::Permit,
        source: api::Source,
    },
    Remove {
        pane: bool,
        change: api::Mutation,
    },
}
impl Intent {
    pub(crate) fn publish(request: &api::Bind, replace: bool) -> Self {
        Self::Publish {
            replace,
            change: request.change.clone(),
            name: request.name.clone(),
            resource_id: request.resource_id,
            rights: request.rights,
            permit: request.permit.clone(),
            source: request.source.clone(),
        }
    }
    fn id(&self) -> OperationId {
        match self {
            Self::Part(request) => request.change.id,
            Self::Publish { change, .. } | Self::Remove { change, .. } => change.id,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    pub via: PendId,
    pub caller: TaskId,
    pub id: OperationId,
}
struct Row {
    key: Key,
    intent: Option<Intent>,
    status: MutationStatus,
}
struct Watermark {
    via: PendId,
    caller: TaskId,
    high: u64,
}
#[derive(Default)]
pub(crate) struct Journal {
    rows: Vec<Row>,
    watermarks: Vec<Watermark>,
}
impl Journal {
    pub(crate) fn begin(
        &mut self,
        via: PendId,
        caller: TaskId,
        intent: Intent,
    ) -> Result<(Key, Option<MutationStatus>), Fail> {
        let key = Key {
            via,
            caller,
            id: intent.id(),
        };
        if let Some(row) = self.rows.iter().find(|row| row.key == key) {
            if row.intent.as_ref() != Some(&intent) {
                return Err(Fail::Conflict);
            }
            return if row.status.state.0 == OperationState::Pending {
                Err(Fail::Unjudged)
            } else {
                Ok((key, Some(row.status.clone())))
            };
        }
        if self.high(key) >= key.id.get() {
            return Err(Fail::Conflict);
        }
        self.reserve(key)?;
        self.rows.push(Row {
            key,
            intent: Some(intent),
            status: status(OperationState::Pending),
        });
        Ok((key, None))
    }
    fn high(&self, key: Key) -> u64 {
        self.watermarks
            .iter()
            .find(|w| w.via == key.via && w.caller == key.caller)
            .map_or(0, |w| w.high)
    }
    fn reserve(&mut self, key: Key) -> Result<(), Fail> {
        let watermark = self
            .watermarks
            .iter()
            .position(|w| w.via == key.via && w.caller == key.caller);
        if watermark.is_none() {
            if self.watermarks.len() >= 256 {
                return Err(Fail::Full);
            }
            self.watermarks.try_reserve(1).map_err(|_| Fail::Full)?;
        }
        let evict = if self.rows.len() >= 256 {
            Some(
                self.rows
                    .iter()
                    .position(|row| row.status.state.0 != OperationState::Pending)
                    .ok_or(Fail::Full)?,
            )
        } else {
            self.rows.try_reserve(1).map_err(|_| Fail::Full)?;
            None
        };
        if let Some(index) = evict {
            self.rows.swap_remove(index);
        }
        if let Some(index) = watermark {
            self.watermarks[index].high = self.watermarks[index].high.max(key.id.get());
        } else {
            self.watermarks.push(Watermark {
                via: key.via,
                caller: key.caller,
                high: key.id.get(),
            });
        }
        Ok(())
    }
    pub(crate) fn finish(&mut self, key: Key, result: Result<Binding, Fail>) -> Result<(), Fail> {
        let row = self
            .rows
            .iter_mut()
            .find(|row| row.key == key)
            .ok_or(Fail::Unknown)?;
        if row.status.state.0 != OperationState::Pending {
            return Err(Fail::Conflict);
        }
        row.status = match result {
            Ok(binding) => MutationStatus {
                state: Coded(OperationState::Applied),
                binding: Optional(Some(binding)),
                failure: Optional(None),
            },
            Err(error) => MutationStatus {
                state: Coded(OperationState::Failed),
                binding: Optional(None),
                failure: Optional(Some(Coded(error))),
            },
        };
        Ok(())
    }
    pub(crate) fn inspect(&self, key: Key) -> MutationStatus {
        self.rows
            .iter()
            .find(|row| row.key == key)
            .map(|row| row.status.clone())
            .unwrap_or_else(|| {
                status(if self.high(key) >= key.id.get() {
                    OperationState::Closed
                } else {
                    OperationState::Unseen
                })
            })
    }
    pub(crate) fn cancel(&mut self, key: Key) -> Result<MutationStatus, Fail> {
        let current = self.inspect(key);
        if current.state.0 != OperationState::Unseen {
            return Ok(current);
        }
        self.reserve(key)?;
        let status = status(OperationState::Closed);
        self.rows.push(Row {
            key,
            intent: None,
            status: status.clone(),
        });
        Ok(status)
    }
    pub(crate) fn forget(&mut self, via: PendId) {
        self.rows.retain(|row| row.key.via != via);
        self.watermarks.retain(|w| w.via != via);
    }
}
fn status(state: OperationState) -> MutationStatus {
    MutationStatus {
        state: Coded(state),
        binding: Optional(None),
        failure: Optional(None),
    }
}

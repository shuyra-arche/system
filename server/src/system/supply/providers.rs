//! Retained, caller-scoped ownership of external registration branches.
//! The original resource remains owned by its provider. Closed proves only the
//! imported Pend branch's withdrawal, never device silence or Task retirement.
use alloc::vec::Vec;
use kernel_api::model::{
    identity::{PendId, TaskId},
    pend::{Distributable, Operations, PendInfo, PendScope, PendStatus},
};
use system_protocol::supply::{Fail, Register, RegistrationId, SourceState, SourceStatus};

const CAPACITY: usize = 64;
enum Phase {
    Active,
    Closing,
    Closed,
}
enum Intent {
    Accepted { request: Register, phase: Phase },
    Cancelled(RegistrationId),
}
struct Registration {
    caller: TaskId,
    intent: Intent,
}
#[derive(Default)]
pub(crate) struct Providers {
    records: Vec<Registration>,
}
impl Providers {
    /// `name` comes from the live Control/Unit association, not the wire.
    /// Inspect is performed at this serial admission boundary, after replay and
    /// capacity decisions. Only a successful commit accepts ownership of offered.
    pub fn register(
        &mut self,
        caller: TaskId,
        name: Option<&str>,
        request: Register,
        inspect: impl FnOnce(PendId) -> Result<PendInfo, Fail>,
    ) -> Result<SourceStatus, Fail> {
        if let Some(old) = self
            .records
            .iter()
            .find(|row| row.caller == caller && row.id() == request.id)
        {
            return match &old.intent {
                Intent::Cancelled(_) => Ok(old.status()),
                Intent::Accepted {
                    request: original, ..
                } if *original == request => Ok(old.status()),
                _ => Err(Fail::Conflict),
            };
        }
        if name != Some(request.source.name.as_str()) || caller.get() == 0 {
            return Err(Fail::Denied);
        }
        if request.offered == system_protocol::PendId::NONE {
            return Err(Fail::Bad);
        }
        let token = PendId::try_from(request.offered).map_err(|_| Fail::Bad)?;
        if !request.source.valid()
            || !request.resource.valid()
            || request.source.name.len() > 128
            || request.resource.name.len() > 128
            || !request.maximum.0.valid()
            || request.maximum.0.scope != PendScope::Object
            || !matches!(
                request.maximum.0.operations,
                Operations::Sign(_) | Operations::Clone(_)
            )
            || request.maximum.0.operations.words()[1] == 0
        {
            return Err(Fail::Bad);
        }
        if self.records.iter().any(|row| match &row.intent {
            Intent::Accepted {
                request: old,
                phase,
            } => {
                old.offered == request.offered
                    || (!matches!(phase, Phase::Closed)
                        && old.source == request.source
                        && old.resource == request.resource)
            }
            Intent::Cancelled(_) => false,
        }) {
            return Err(Fail::Conflict);
        }
        if self.records.len() == CAPACITY {
            return Err(Fail::Full);
        }
        self.records.try_reserve(1).map_err(|_| Fail::Full)?;
        let info = inspect(token)?;
        let mut derivation = request.maximum.0;
        derivation.distributable = Distributable::Yes;
        if info.token != token
            || info.lender != caller
            || info.status != PendStatus::Active
            || !info.limits.permits_narrowing(derivation)
        {
            return Err(Fail::Denied);
        }
        self.records.push(Registration {
            caller,
            intent: Intent::Accepted {
                request,
                phase: Phase::Active,
            },
        });
        Ok(self.records.last().unwrap().status())
    }
    pub fn inspect(&self, caller: TaskId, id: RegistrationId) -> Result<SourceStatus, Fail> {
        self.records
            .iter()
            .find(|row| row.caller == caller && row.id() == id)
            .map(Registration::status)
            .ok_or(Fail::Unknown)
    }
    pub fn withdraw(&mut self, caller: TaskId, id: RegistrationId) -> Result<SourceStatus, Fail> {
        if let Some(row) = self
            .records
            .iter_mut()
            .find(|row| row.caller == caller && row.id() == id)
        {
            if let Intent::Accepted { phase, .. } = &mut row.intent
                && matches!(phase, Phase::Active)
            {
                *phase = Phase::Closing;
            }
            return Ok(row.status());
        }
        // An unseen Withdraw must prevent a delayed Register from reviving the
        // ID. Cancelled accepts no input-grant ownership; the provider retains
        // and retires any grant it had already offered for that losing request.
        if caller.get() == 0 {
            return Err(Fail::Denied);
        }
        if self.records.len() == CAPACITY {
            return Err(Fail::Full);
        }
        self.records.try_reserve(1).map_err(|_| Fail::Full)?;
        self.records.push(Registration {
            caller,
            intent: Intent::Cancelled(id),
        });
        Ok(self.records.last().unwrap().status())
    }
    pub fn find(
        &self,
        source: &str,
        resource: &str,
    ) -> Option<(PendId, kernel_api::model::pend::Limits)> {
        self.records.iter().find_map(|row| match &row.intent {
            Intent::Accepted {
                request,
                phase: Phase::Active,
            } if request.source.name == source && request.resource.name == resource => Some((
                PendId::try_from(request.offered).unwrap(),
                request.maximum.0,
            )),
            _ => None,
        })
    }
    /// Loss of a provider or grant closes admission. Only the executor's actual
    /// Return/strict missing-record completion may commit Closed. Busy and errors
    /// retain the same root; old records remain for replay and token alias checks.
    pub fn advance(
        &mut self,
        mut lost: impl FnMut(TaskId, PendId) -> bool,
        mut retire: impl FnMut(PendId) -> Result<bool, Fail>,
    ) -> Result<(), Fail> {
        for row in &mut self.records {
            let Intent::Accepted { request, phase } = &mut row.intent else {
                continue;
            };
            if matches!(phase, Phase::Closed) {
                continue;
            }
            let token = PendId::try_from(request.offered).unwrap();
            if matches!(phase, Phase::Active) && lost(row.caller, token) {
                *phase = Phase::Closing;
            }
            if matches!(phase, Phase::Closing) && retire(token)? {
                *phase = Phase::Closed;
            }
        }
        Ok(())
    }
    pub fn close(&mut self) {
        for row in &mut self.records {
            if let Intent::Accepted { phase, .. } = &mut row.intent
                && matches!(phase, Phase::Active)
            {
                *phase = Phase::Closing;
            }
        }
    }
    pub fn closed(&self) -> bool {
        self.records.iter().all(|row| {
            matches!(
                row.intent,
                Intent::Cancelled(_)
                    | Intent::Accepted {
                        phase: Phase::Closed,
                        ..
                    }
            )
        })
    }
}

impl Registration {
    fn id(&self) -> RegistrationId {
        match &self.intent {
            Intent::Accepted { request, .. } => request.id,
            Intent::Cancelled(id) => *id,
        }
    }
    fn status(&self) -> SourceStatus {
        let state = match &self.intent {
            Intent::Accepted {
                phase: Phase::Active,
                ..
            } => SourceState::Active,
            Intent::Accepted {
                phase: Phase::Closing,
                ..
            } => SourceState::Closing,
            Intent::Accepted {
                phase: Phase::Closed,
                ..
            } => SourceState::Closed,
            Intent::Cancelled(_) => SourceState::Cancelled,
        };
        SourceStatus {
            id: self.id(),
            state,
        }
    }
}

//! Bounded native Clone subscriptions. Events are best effort, as in the
//! original contract: full queues can leave sequence gaps, without blocking
//! namespace mutations. Cleanup retains ownership until native Return succeeds.
use alloc::vec::Vec;
use kernel_api::{
    address::VirtAddr,
    model::{
        identity::PendId,
        pend::{CloneRights, Operations, PendInfo, PendStatus},
    },
};
use kernel_client::call::pend;
use system_protocol::{
    Path, TaskId,
    operator::{Event, Fail},
};
use twine::view::clone::Sender;
use weave::{Codec, transport};
enum Phase {
    Candidate,
    Live,
    Closing,
}
struct Watch {
    caller: TaskId,
    token: PendId,
    path: Path,
    phase: Phase,
}
pub(crate) struct Watchers {
    rows: Vec<Watch>,
    bytes: Vec<u8>,
}
impl Watchers {
    pub(crate) fn new() -> Result<Self, Fail> {
        let mut rows = Vec::new();
        rows.try_reserve_exact(257).map_err(|_| Fail::Full)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(Event::SIZE.maximum().unwrap())
            .map_err(|_| Fail::Full)?;
        bytes.resize(Event::SIZE.maximum().unwrap(), 0);
        Ok(Self { rows, bytes })
    }
    pub(crate) fn blocked(&self) -> bool {
        self.rows.len() > 256
    }
    pub(crate) fn contains(&self, token: PendId) -> bool {
        self.rows.iter().any(|row| row.token == token)
    }
    pub(crate) fn join(&mut self, caller: TaskId, token: PendId, path: Path) -> Result<(), Fail> {
        if token == PendId::NONE || self.contains(token) {
            return Err(Fail::Conflict);
        }
        if self.blocked() {
            return Err(Fail::Full);
        }
        self.rows.push(Watch {
            caller,
            token,
            path,
            phase: Phase::Candidate,
        });
        let at = self.rows.len() - 1;
        let info = match inspect(token) {
            Ok(info) => info,
            Err(Fail::Unknown) => {
                self.rows.swap_remove(at);
                return Err(Fail::Unknown);
            }
            Err(error) => return Err(error),
        };
        if info.lender.get() as u64 != caller.get() {
            self.rows.swap_remove(at);
            return Err(Fail::Denied);
        }
        self.rows[at].phase = Phase::Closing;
        if self.rows.len() > 256 {
            return Err(Fail::Full);
        }
        if info.owner.get() as u64 != caller.get()
            || info.status != PendStatus::Active
            || !matches!(info.limits.operations, Operations::Clone(rights) if rights.contains(CloneRights::PUSH))
            || info.limits.modifiable != kernel_api::model::modifiable::Modifiable::Yes
        {
            return Err(Fail::Denied);
        }
        if self.rows[..at].iter().any(|row| {
            row.caller == caller
                && row.path == self.rows[at].path
                && matches!(row.phase, Phase::Live)
        }) {
            return Err(Fail::Conflict);
        }
        self.rows[at].phase = Phase::Live;
        Ok(())
    }
    pub(crate) fn publish(&mut self, event: &Event) {
        for row in &mut self.rows {
            if !matches!(row.phase, Phase::Live) || !prefix(&row.path, &event.path) {
                continue;
            }
            match transport::send(&mut Sender::new(row.token), event, &mut self.bytes) {
                Ok(()) => (),
                Err(transport::SendError::Transport(error)) if error.source.is_busy() => (),
                Err(_) => row.phase = Phase::Closing,
            }
        }
    }
    pub(crate) fn sweep(&mut self) -> bool {
        let mut at = 0;
        while at < self.rows.len() {
            let row = &mut self.rows[at];
            match inspect(row.token) {
                Ok(info)
                    if matches!(row.phase, Phase::Candidate)
                        && info.lender.get() as u64 != row.caller.get() =>
                {
                    self.rows.swap_remove(at);
                    continue;
                }
                Ok(info) => {
                    if matches!(row.phase, Phase::Candidate) || info.status != PendStatus::Active {
                        row.phase = Phase::Closing;
                    }
                }
                Err(Fail::Unknown) => {
                    self.rows.swap_remove(at);
                    continue;
                }
                Err(_) => {
                    at += 1;
                    continue;
                }
            }
            if matches!(row.phase, Phase::Closing) && pend::return_(row.token).is_ok() {
                self.rows.swap_remove(at);
            } else {
                at += 1;
            }
        }
        self.rows
            .iter()
            .any(|row| !matches!(row.phase, Phase::Live))
    }
    pub(crate) fn finish(&mut self) -> Result<(), Fail> {
        for row in &mut self.rows {
            if !matches!(row.phase, Phase::Candidate) {
                row.phase = Phase::Closing;
            }
        }
        self.sweep();
        if self.rows.is_empty() {
            Ok(())
        } else {
            Err(Fail::Unjudged)
        }
    }
}
fn prefix(base: &Path, path: &Path) -> bool {
    base.as_str().is_empty()
        || path
            .as_str()
            .strip_prefix(base.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}
fn inspect(token: PendId) -> Result<PendInfo, Fail> {
    let mut words = [0; PendInfo::WORDS];
    pend::inspect(token, VirtAddr::new(words.as_mut_ptr().addr())).map_err(|error| {
        if error.source.is_busy() {
            Fail::Unjudged
        } else {
            Fail::Unknown
        }
    })?;
    PendInfo::from_words(words).ok_or(Fail::Bad)
}

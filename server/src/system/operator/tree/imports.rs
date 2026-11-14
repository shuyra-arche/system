//! Owned publication roots. Unbinding retains delivered children; authority
//! withdrawal retires the whole native branch. Partial returns remain owned.
use crate::system::operator::authority::GrantKey;
use alloc::vec::Vec;
use kernel_api::{
    address::VirtAddr,
    model::{
        identity::PendId,
        pend::{AccordRequest, AccordTarget, PendInfo, PendStatus},
    },
};
use kernel_client::call::pend;
use system_protocol::{
    TaskId,
    operator::{Fail, Source},
    value::Rights,
};

const LIMIT: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Key(usize);
#[derive(Clone, Copy)]
enum Phase {
    Candidate(TaskId),
    Bound,
    Retained,
    Withdrawing,
}
struct Root {
    token: PendId,
    authority: GrantKey,
    phase: Phase,
    source: Source,
    rights: Rights,
}
pub(super) struct Imports {
    roots: Vec<Option<Root>>,
}
impl Imports {
    pub(super) fn new() -> Result<Self, Fail> {
        let mut roots = Vec::new();
        roots.try_reserve_exact(LIMIT + 1).map_err(|_| Fail::Full)?;
        roots.resize_with(LIMIT + 1, || None);
        Ok(Self { roots })
    }
    pub(super) fn blocked(&self) -> bool {
        self.roots[LIMIT].is_some()
    }
    pub(super) fn contains(&self, token: PendId) -> bool {
        self.roots.iter().flatten().any(|r| r.token == token)
    }
    /// Protected service grants are rejected by the caller before this method.
    /// Candidate inspection may fail transiently; the ledger retains that
    /// candidate until its lender can be verified and cleanup can finish.
    pub(super) fn admit(
        &mut self,
        token: PendId,
        caller: TaskId,
        authority: GrantKey,
        source: Source,
        rights: Rights,
    ) -> Result<Key, Fail> {
        if token == PendId::NONE || self.contains(token) {
            return Err(Fail::Conflict);
        }
        let at = self
            .roots
            .iter()
            .position(Option::is_none)
            .ok_or(Fail::Full)?;
        self.roots[at] = Some(Root {
            token,
            authority,
            phase: Phase::Candidate(caller),
            source,
            rights,
        });
        let info = match inspect(token) {
            Ok(info) => info,
            Err(Fail::Unknown) => {
                self.roots[at] = None;
                return Err(Fail::Unknown);
            }
            Err(error) => return Err(error),
        };
        let root = self.roots[at].as_mut().unwrap();
        if info.lender.get() as u64 != caller.get() {
            // A guessed coordinate cannot transfer ownership of another
            // caller's grant, and must never make us Return that grant.
            self.roots[at] = None;
            return Err(Fail::Denied);
        }
        root.phase = Phase::Withdrawing;
        if info.status != PendStatus::Active
            || info.owner.get() as u64 != root.source.task.get()
            || info.mark.get() != root.source.mark.get()
            || !info.limits.permits_child(rights.0)
        {
            return Err(Fail::Denied);
        }
        if at == LIMIT {
            return Err(Fail::Full);
        }
        root.phase = Phase::Bound;
        Ok(Key(at))
    }
    pub(super) fn reject(&mut self, key: Key) {
        if let Some(root) = &mut self.roots[key.0] {
            root.phase = Phase::Withdrawing;
        }
    }
    pub(super) fn retain(&mut self, key: Key) {
        if let Some(root) = &mut self.roots[key.0] {
            root.phase = Phase::Retained;
        }
    }
    pub(super) fn source(&self, key: Key) -> Result<TaskId, Fail> {
        let root = self.root(key)?;
        let info = inspect(root.token).map_err(|error| {
            if error == Fail::Unknown {
                Fail::Dead
            } else {
                error
            }
        })?;
        if info.status != PendStatus::Active {
            return Err(Fail::Dead);
        }
        Ok(info.owner.into())
    }
    pub(super) fn send(&self, key: Key, caller: TaskId) -> Result<system_protocol::PendId, Fail> {
        let root = self.root(key)?;
        let info = inspect(root.token).map_err(|error| {
            if error == Fail::Unknown {
                Fail::Dead
            } else {
                error
            }
        })?;
        if info.status != PendStatus::Active {
            return Err(Fail::Dead);
        }
        let request = AccordRequest {
            src: root.token,
            target: AccordTarget::Task(caller.try_into().map_err(|_| Fail::Bad)?),
            mark: kernel_api::model::identity::Mark::NONE,
            limits: root.rights.0,
        };
        pend::accord(VirtAddr::new(request.to_bytes().as_ptr().addr()))
            .map(Into::into)
            .map_err(|error| {
                if error.source.is_busy() {
                    Fail::Unjudged
                } else {
                    Fail::Denied
                }
            })
    }
    fn root(&self, key: Key) -> Result<&Root, Fail> {
        self.roots
            .get(key.0)
            .and_then(Option::as_ref)
            .filter(|r| matches!(r.phase, Phase::Bound))
            .ok_or(Fail::Dead)
    }
    pub(super) fn withdraw(&mut self, authority: GrantKey) {
        for root in self.roots.iter_mut().flatten() {
            if root.authority == authority && !matches!(root.phase, Phase::Candidate(_)) {
                root.phase = Phase::Withdrawing;
            }
        }
    }
    /// One bounded cleanup pass. A failed native inspection/Return retains
    /// the exact record; it cannot become a successful reclamation receipt.
    pub(super) fn sweep(&mut self) -> bool {
        let mut complete = true;
        for cell in &mut self.roots {
            let Some(root) = cell else { continue };
            if let Phase::Candidate(caller) = root.phase {
                match inspect(root.token) {
                    Ok(info) if info.lender.get() as u64 == caller.get() => {
                        root.phase = Phase::Withdrawing
                    }
                    Ok(_) => {
                        *cell = None;
                        continue;
                    }
                    Err(Fail::Unknown) => {
                        *cell = None;
                        continue;
                    }
                    Err(_) => {
                        complete = false;
                        continue;
                    }
                }
            }
            if matches!(root.phase, Phase::Retained) {
                match inspect(root.token) {
                    Ok(info) if info.status == PendStatus::Dead => root.phase = Phase::Withdrawing,
                    Ok(_) => continue,
                    Err(Fail::Unknown) => {
                        *cell = None;
                        continue;
                    }
                    Err(_) => {
                        complete = false;
                        continue;
                    }
                }
            }
            if matches!(root.phase, Phase::Withdrawing) {
                match pend::return_(root.token) {
                    Ok(()) => *cell = None,
                    Err(error)
                        if !error.source.is_busy()
                            && matches!(inspect(root.token), Err(Fail::Unknown)) =>
                    {
                        *cell = None
                    }
                    Err(_) => complete = false,
                }
            }
        }
        complete
    }
    pub(super) fn finish(&mut self) -> Result<(), Fail> {
        for root in self.roots.iter_mut().flatten() {
            if !matches!(root.phase, Phase::Candidate(_)) {
                root.phase = Phase::Withdrawing;
            }
        }
        if self.sweep() && self.roots.iter().all(Option::is_none) {
            Ok(())
        } else {
            Err(Fail::Unjudged)
        }
    }
    pub(super) fn settled(&self, authority: GrantKey) -> bool {
        !self
            .roots
            .iter()
            .flatten()
            .any(|r| r.authority == authority)
    }
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

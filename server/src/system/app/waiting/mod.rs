//! Owned supervisor subscriptions. The recorded set follows successful Kernel
//! changes, including partial reconciliation and retryable shutdown.
use alloc::vec::Vec;
use erra::Error as KernelError;
use kernel_api::{
    abi::call::{
        pend::PendFail,
        sign::{SignFail, SignReply},
    },
    model::{clone_view::CloneCondition, identity::PendId, sign::SignSource, wait::Wait},
};
use kernel_client::call::{pend, sign};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interest {
    pub source: SignSource,
    pub generation: u64,
}
impl Interest {
    pub(crate) const fn new(source: SignSource) -> Self {
        Self {
            source,
            generation: 0,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Create(KernelError<SignFail>),
    Attach {
        interest: Interest,
        error: KernelError<SignFail>,
    },
    Detach {
        interest: Interest,
        error: KernelError<SignFail>,
    },
    Listen(KernelError<SignFail>),
    Return(KernelError<PendFail>),
    Capacity,
    Conflict,
    Inactive,
}

#[must_use = "finish the wait group and retry any failed return"]
pub(crate) struct Waiting {
    token: Option<PendId>,
    sources: Vec<Interest>,
    closing: bool,
}
impl Waiting {
    pub(crate) fn new(license: PendId) -> Result<Self, Error> {
        let token = sign::create(license).map_err(Error::Create)?;
        Ok(Self {
            token: Some(token),
            sources: Vec::new(),
            closing: false,
        })
    }

    pub(crate) fn await_(&self, wait: Wait) -> Result<SignReply, Error> {
        sign::listen(self.active()?, wait).map_err(Error::Listen)
    }

    /// Reserve before changing subscriptions. On a failed detach, keep the
    /// record. On a failed attach, retain all earlier successful changes.
    pub(crate) fn apply(
        &mut self,
        reads: &[PendId],
        writes: &[PendId],
        extra: &[Interest],
    ) -> Result<(), Error> {
        let mut desired = Vec::new();
        desired
            .try_reserve(
                reads
                    .len()
                    .checked_add(writes.len())
                    .and_then(|n| n.checked_add(extra.len()))
                    .ok_or(Error::Capacity)?,
            )
            .map_err(|_| Error::Capacity)?;
        for interest in reads
            .iter()
            .map(|&token| {
                Interest::new(SignSource::Clone {
                    token,
                    condition: CloneCondition::Pull,
                })
            })
            .chain(writes.iter().map(|&token| {
                Interest::new(SignSource::Clone {
                    token,
                    condition: CloneCondition::Push,
                })
            }))
            .chain(extra.iter().copied())
        {
            if let Some(previous) = desired
                .iter()
                .find(|old: &&Interest| old.source == interest.source)
            {
                if previous.generation != interest.generation {
                    return Err(Error::Conflict);
                }
            } else {
                desired.push(interest);
            }
        }
        self.replace(&desired)
    }

    fn replace(&mut self, desired: &[Interest]) -> Result<(), Error> {
        let token = self.active()?;
        self.sources
            .try_reserve(desired.len())
            .map_err(|_| Error::Capacity)?;
        let mut at = 0;
        while at < self.sources.len() {
            if desired.contains(&self.sources[at]) {
                at += 1;
            } else {
                let interest = self.sources[at];
                sign::detach(token, interest.source)
                    .map_err(|error| Error::Detach { interest, error })?;
                self.sources.swap_remove(at);
            }
        }
        for &interest in desired {
            if !self.sources.contains(&interest) {
                sign::attach(token, interest.source, interest.generation)
                    .map_err(|error| Error::Attach { interest, error })?;
                self.sources.push(interest);
            }
        }
        Ok(())
    }

    pub(crate) fn detach(&mut self, token: PendId) -> Result<(), Error> {
        let group = self.active()?;
        let mut at = 0;
        while at < self.sources.len() {
            let interest = self.sources[at];
            if matches!(interest.source, SignSource::Clone { token: source, .. } if source == token)
            {
                sign::detach(group, interest.source)
                    .map_err(|error| Error::Detach { interest, error })?;
                self.sources.swap_remove(at);
            } else {
                at += 1;
            }
        }
        Ok(())
    }

    fn active(&self) -> Result<PendId, Error> {
        if self.closing {
            return Err(Error::Inactive);
        }
        self.token.ok_or(Error::Inactive)
    }

    pub(crate) fn finish(&mut self) -> Result<(), Error> {
        self.closing = true;
        if let Some(token) = self.token {
            pend::return_(token).map_err(Error::Return)?;
            self.token = None;
            self.sources.clear();
        }
        Ok(())
    }
}
impl Drop for Waiting {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

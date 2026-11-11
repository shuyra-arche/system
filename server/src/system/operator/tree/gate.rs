//! Namespace admission depends on one trusted query bundle. Unknown facts fail
//! closed; Same is one atomic identity query rather than two Resolve snapshots.
use kernel_api::model::wait::Wait;
use system_client::identity::TaskQuery;
use system_protocol::{
    EntryId, TaskId,
    identity::{self, Selector},
    operator::{Fail, Permit},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unavailable;

pub trait Facts {
    fn bound(&self, task: TaskId) -> Result<bool, Unavailable>;
    fn matches(&self, task: TaskId, selector: Selector) -> Result<bool, Unavailable>;
    fn same(&self, left: TaskId, right: TaskId) -> Result<bool, Unavailable>;
    fn opens(&self, entry: EntryId) -> Result<Option<TaskId>, Unavailable>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    Ok,
    Denied,
    Unjudged,
}
impl Code {
    pub const fn passed(self) -> bool {
        matches!(self, Self::Ok)
    }
    pub const fn failure(self) -> Option<Fail> {
        match self {
            Self::Ok => None,
            Self::Denied => Some(Fail::Denied),
            Self::Unjudged => Some(Fail::Unjudged),
        }
    }
}

pub fn verdict(facts: &impl Facts, who: TaskId, permit: Permit) -> Code {
    let answer = match permit {
        Permit::Public => return Code::Ok,
        Permit::Bound => facts.bound(who),
        Permit::Identity(selector) => facts.matches(who, selector),
        Permit::Opener(at) => match facts.opens(at) {
            Ok(Some(opener)) => facts.same(who, opener),
            Ok(None) | Err(Unavailable) => return Code::Unjudged,
        },
    };
    match answer {
        Ok(true) => Code::Ok,
        Ok(false) => Code::Denied,
        Err(Unavailable) => Code::Unjudged,
    }
}

/// The opener lookup comes from the serving Operator's own namespace state.
/// Query endpoints come from an authenticated bootstrap receipt. Every query
/// uses the same supplied absolute deadline and rechecks live native authority.
pub struct Court<'a, F> {
    pub query: Option<&'a TaskQuery>,
    pub openers: F,
    pub wait: Wait,
}
impl<F: Fn(EntryId) -> Result<Option<TaskId>, Unavailable>> Facts for Court<'_, F> {
    fn bound(&self, task: TaskId) -> Result<bool, Unavailable> {
        self.query
            .ok_or(Unavailable)?
            .resolve(identity::Task { task }, self.wait)
            .map(|value| value.is_some())
            .map_err(|_| Unavailable)
    }
    fn matches(&self, task: TaskId, selector: Selector) -> Result<bool, Unavailable> {
        self.query
            .ok_or(Unavailable)?
            .matches(identity::Matches { task, selector }, self.wait)
            .map(|value| value == identity::Match::Yes)
            .map_err(|_| Unavailable)
    }
    fn same(&self, left: TaskId, right: TaskId) -> Result<bool, Unavailable> {
        self.query
            .ok_or(Unavailable)?
            .same(identity::Same { left, right }, self.wait)
            .map(|value| value == identity::Match::Yes)
            .map_err(|_| Unavailable)
    }
    fn opens(&self, at: EntryId) -> Result<Option<TaskId>, Unavailable> {
        (self.openers)(at)
    }
}

#[cfg(test)]
#[path = "../../../../../tests/operator-policy/policy.rs"]
mod tests;

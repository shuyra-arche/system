use super::*;

struct Offline;
impl Facts for Offline {
    fn bound(&self, _: TaskId) -> Result<bool, Unavailable> {
        Err(Unavailable)
    }
    fn matches(&self, _: TaskId, _: Selector) -> Result<bool, Unavailable> {
        Err(Unavailable)
    }
    fn same(&self, _: TaskId, _: TaskId) -> Result<bool, Unavailable> {
        Err(Unavailable)
    }
    fn opens(&self, _: EntryId) -> Result<Option<TaskId>, Unavailable> {
        Err(Unavailable)
    }
}

#[test]
fn offline_is_public_only() {
    let who = TaskId::new(7);
    assert_eq!(verdict(&Offline, who, Permit::Public), Code::Ok);
    assert_eq!(verdict(&Offline, who, Permit::Bound), Code::Unjudged);
    assert_eq!(
        verdict(&Offline, who, Permit::Opener(EntryId::new(0))),
        Code::Unjudged
    );
}

struct Answers {
    bound: Result<bool, Unavailable>,
    matches: Result<bool, Unavailable>,
    same: Result<bool, Unavailable>,
    opener: Option<TaskId>,
}

impl Facts for Answers {
    fn bound(&self, _: TaskId) -> Result<bool, Unavailable> {
        self.bound
    }
    fn matches(&self, _: TaskId, _: Selector) -> Result<bool, Unavailable> {
        self.matches
    }
    fn same(&self, _: TaskId, _: TaskId) -> Result<bool, Unavailable> {
        self.same
    }
    fn opens(&self, _: EntryId) -> Result<Option<TaskId>, Unavailable> {
        Ok(self.opener)
    }
}

#[test]
fn bound_distinguishes_no_binding_from_unavailable() {
    let facts = Answers {
        bound: Ok(false),
        matches: Err(Unavailable),
        same: Err(Unavailable),
        opener: None,
    };
    assert_eq!(verdict(&facts, TaskId::new(1), Permit::Bound), Code::Denied);
}

#[test]
fn identity_uses_one_atomic_match_not_resolve() {
    use system_protocol::identity::PrincipalId;
    let selector = Selector::Exact(PrincipalId {
        authority: TaskId::new(9),
        slot: 0,
    });
    let mut facts = Answers {
        bound: Err(Unavailable),
        matches: Ok(true),
        same: Err(Unavailable),
        opener: None,
    };
    assert_eq!(
        verdict(&facts, TaskId::new(1), Permit::Identity(selector.clone())),
        Code::Ok
    );
    facts.matches = Ok(false);
    assert_eq!(
        verdict(&facts, TaskId::new(1), Permit::Identity(selector.clone())),
        Code::Denied
    );
    facts.matches = Err(Unavailable);
    assert_eq!(
        verdict(&facts, TaskId::new(1), Permit::Identity(selector.clone())),
        Code::Unjudged
    );
}

#[test]
fn opener_uses_same_not_two_resolves() {
    let mut facts = Answers {
        bound: Err(Unavailable),
        matches: Err(Unavailable),
        same: Ok(true),
        opener: Some(TaskId::new(2)),
    };
    let permit = Permit::Opener(EntryId::new(0));
    assert_eq!(verdict(&facts, TaskId::new(1), permit.clone()), Code::Ok);
    facts.same = Ok(false);
    assert_eq!(
        verdict(&facts, TaskId::new(1), permit.clone()),
        Code::Denied
    );
    facts.same = Err(Unavailable);
    assert_eq!(
        verdict(&facts, TaskId::new(1), permit.clone()),
        Code::Unjudged
    );
    facts.opener = None;
    assert_eq!(
        verdict(&facts, TaskId::new(1), permit.clone()),
        Code::Unjudged
    );
}

use alloc::{collections::VecDeque, vec::Vec};
use core::cell::RefCell;
use kernel_api::model::{
    identity::{Mark, PendId, TaskId, TeamId},
    modifiable::Modifiable,
    pend::{Distributable, Limits, Operations, PendInfo, PendScope, Target},
    sign::SignRights,
    unit::{UnitInfo, UnitState, UnitTarget},
};
use system_protocol::{
    operator::{self as op, authority::operations as authority, operations as operator},
    value::{Optional, Outcome},
};
use weave::{Codec, Contract, Operation};

const CALLER: TaskId = TaskId::new(2);
const OPERATOR: TaskId = TaskId::new(9);

#[derive(Default)]
struct Requests {
    authority: Vec<authority::Request>,
    operator: Vec<operator::Request>,
}

pub struct Services {
    pub caller: TaskId,
    script: RefCell<VecDeque<Vec<u8>>>,
    requests: RefCell<Requests>,
}

impl Services {
    pub fn new() -> Self {
        Self {
            caller: CALLER,
            script: RefCell::new(VecDeque::new()),
            requests: RefCell::new(Requests::default()),
        }
    }
    pub fn operator(&self) -> TaskId {
        OPERATOR
    }
    pub fn reply(&self, response: authority::Response) {
        // These encoded service replies exercise System's receipt handling;
        // they do not assert what a native Operator would acknowledge.
        self.script.borrow_mut().push_back(encode(response));
    }
    pub fn request_log(&self) -> Vec<authority::Request> {
        core::mem::take(&mut self.requests.borrow_mut().authority)
    }
    pub fn mutate<O>(&self, body: O::Request) -> <<O as Contract>::Response as Codec>::Decoded
    where
        O: Operation<Interface = authority::Interface>,
    {
        self.requests
            .borrow_mut()
            .authority
            .push(O::wrap_request(body));
        let bytes = self
            .script
            .borrow_mut()
            .pop_front()
            .expect("the test must script every typed authority reply");
        let (response, used) =
            <<O::Interface as Contract>::Response as Codec>::decode(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        O::unwrap_response(response).expect("response must match its typed operation")
    }
    pub fn query<O>(&self, _body: O::Request) -> <<O as Contract>::Response as Codec>::Decoded
    where
        O: Operation<Interface = operator::Interface>,
    {
        panic!("the instance namespace fixture must not query global layout")
    }
}

fn encode<T: Codec>(response: T) -> Vec<u8> {
    let size = T::SIZE.maximum().expect("bounded protocol response");
    let mut bytes = alloc::vec![0; size];
    let used = response
        .encode(&mut bytes)
        .expect("valid protocol response");
    bytes.truncate(used);
    bytes
}

pub fn binding(id: usize) -> op::Binding {
    op::Binding {
        entry: op::Entry {
            operator: OPERATOR.into(),
            target: op::EntryTarget::Node(system_protocol::EntryId::new(id as u64)),
        },
        revision: 0,
    }
}

pub fn root(instance: u64, pane: op::Binding, task: TaskId) -> op::unit::Root {
    op::unit::Root {
        registration: op::unit::Registration {
            instance: system_protocol::control::instance::InstanceId {
                system: CALLER.into(),
                sequence: system_protocol::control::instance::Sequence::new(instance).unwrap(),
            },
            owner: task.into(),
            task: task.into(),
            team: system_protocol::TeamId::new(task.get() as u64 + 20),
            pane,
        },
        revision: 0,
        phase: op::unit::Phase::Preparing,
    }
}

pub fn queue_install(
    services: &Services,
    instance: u64,
    task: TaskId,
    publication: bool,
    grant: Outcome<op::GrantStatus, op::Fail>,
) {
    let pane = binding(instance as usize * 10 + 1);
    services.reply(authority::Response::Part(Outcome::Success(pane.clone())));
    services.reply(authority::Response::RegisterRoot(Outcome::Success(root(
        instance, pane, task,
    ))));
    let namespace = binding(instance as usize * 10 + 2);
    services.reply(authority::Response::Part(Outcome::Success(
        namespace.clone(),
    )));
    if publication {
        services.reply(authority::Response::Bind(Outcome::Success(namespace)));
    }
    services.reply(authority::Response::Grant(grant));
}

pub fn grant_status(id: op::GrantId, state: op::GrantState) -> op::GrantStatus {
    op::GrantStatus {
        state: weave::Coded(state),
        grant: id,
        entrance: Optional(None),
        binding: Optional(None),
    }
}

pub fn mutation_status(state: op::OperationState) -> op::MutationStatus {
    op::MutationStatus {
        state: weave::Coded(state),
        binding: Optional(None),
        failure: Optional(None),
    }
}

#[derive(Default)]
struct Effects {
    next_root: usize,
    releases: Vec<PendId>,
    release_results: VecDeque<bool>,
    givens: Vec<(PendId, TaskId)>,
}
std::thread_local! {
    static EFFECTS: RefCell<Effects> = RefCell::new(Effects::default());
}

pub fn reset_effects(release_results: impl IntoIterator<Item = bool>) {
    EFFECTS.with(|cell| {
        *cell.borrow_mut() = Effects {
            release_results: release_results.into_iter().collect(),
            ..Effects::default()
        }
    });
}
pub fn release_log() -> Vec<PendId> {
    EFFECTS.with(|cell| cell.borrow().releases.clone())
}
pub fn give_log() -> Vec<(PendId, TaskId)> {
    EFFECTS.with(|cell| cell.borrow().givens.clone())
}

pub fn lookup(task: TaskId) -> Option<UnitInfo> {
    Some(UnitInfo {
        target: UnitTarget::Task(task),
        team: TeamId::new(task.get() + 20),
        controller: CALLER,
        creator: CALLER,
        sire: CALLER,
        state: UnitState::Running,
        reclaimed: false,
        admitting: true,
        exit: None,
        result: None,
    })
}

pub fn inspect(token: PendId) -> PendInfo {
    PendInfo {
        token,
        target: Target::Sign(kernel_api::model::sign::SignId::new(token.get() + 1)),
        limits: Limits {
            operations: Operations::Sign(SignRights::TRIGGER),
            modifiable: Modifiable::No,
            distributable: Distributable::No,
            scope: PendScope::Object,
        },
        owner: CALLER,
        lender: TaskId::new(0),
        mark: Mark::new(700),
        status: kernel_api::model::pend::PendStatus::Active,
    }
}

pub fn give(
    source: PendId,
    target: TaskId,
    _mark: Mark,
    _limits: Limits,
) -> Result<PendId, kernel_api::abi::call::pend::PendFail> {
    EFFECTS.with(|cell| cell.borrow_mut().givens.push((source, target)));
    Ok(PendId::mint(source.get() + 1000))
}
pub fn release(token: PendId) -> bool {
    EFFECTS.with(|cell| {
        let mut state = cell.borrow_mut();
        state.releases.push(token);
        state.release_results.pop_front().unwrap_or(true)
    })
}
pub fn fatal(note: &'static str) -> ! {
    panic!("unexpected fatal service effect: {note}")
}
pub const LIMIT_NS: u64 = 5_000_000_000;

pub fn bound(_deadline: u64, note: &'static str) {
    panic!("unexpected wait in namespace effect script: {note}")
}

pub fn accord(_address: kernel_api::address::VirtAddr) -> Result<PendId, ()> {
    EFFECTS.with(|cell| {
        let mut state = cell.borrow_mut();
        state.next_root += 1;
        Ok(PendId::mint(500 + state.next_root))
    })
}
pub fn clock() -> u64 {
    1
}

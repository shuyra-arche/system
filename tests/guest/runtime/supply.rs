//! Actual Kernel grant derivation/withdrawal; full Supply RPC wiring is separate.
use crate::{
    Console, inspect,
    native_supply::{Resource, Source},
};
use alloc::string::String;
use core::fmt::Write;
use kernel_api::{
    address::VirtAddr,
    model::{
        clone_view::CloneLimits,
        identity::{Mark, PendId, TaskId},
        modifiable::Modifiable,
        pend::{
            CloneRights, Distributable, Limits, Operations, PendInfo, PendScope, PendStatus, Target,
        },
        unit::{UnitQuery, UnitTarget},
    },
};
use kernel_client::call::{clone_view, pend, room};
use system_protocol::{
    supply::{self as api, Fail, LeaseId, State},
    value::Rights,
};
fn limits() -> Limits {
    Limits {
        operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
        modifiable: Modifiable::No,
        distributable: Distributable::No,
        scope: PendScope::Object,
    }
}
fn request(id: u64, root: TaskId) -> api::Acquire {
    api::Acquire {
        lease: LeaseId::new(id).unwrap(),
        target: system_protocol::TaskId::new(root.get() as u64),
        binding: api::Binding {
            source: api::Key {
                name: String::from("system/test"),
            },
            resource: api::Key {
                name: String::from("queue"),
            },
            slot: system_protocol::Mark::new(91),
            rights: Rights(limits()),
        },
    }
}
fn info(token: PendId) -> PendInfo {
    let mut bytes = [0; PendInfo::SIZE];
    pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())).unwrap();
    PendInfo::from_bytes(&bytes).unwrap()
}
fn related(target: Target) -> usize {
    let mut after = PendId::NONE;
    let mut count = 0;
    loop {
        let mut bytes = [0; PendInfo::SIZE];
        if pend::collect(after, VirtAddr::new(bytes.as_mut_ptr().addr()), 1).unwrap() == 0 {
            return count;
        }
        let value = PendInfo::from_bytes(&bytes).unwrap();
        if value.target == target && value.status == PendStatus::Active {
            count += 1;
        }
        after = value.token;
    }
}
fn step(source: &mut Source, root: TaskId, id: u64, state: State) -> api::Reply {
    let deadline = room::clock() + 5_000_000_000;
    loop {
        source.advance().unwrap();
        let reply = source.inspect(root, LeaseId::new(id).unwrap()).unwrap();
        if reply.state == state {
            return reply;
        }
        assert!(room::clock() < deadline);
        room::yield_();
    }
}
pub fn run(license: PendId) {
    let UnitTarget::Task(root) = inspect(UnitQuery::Caller).target else {
        panic!("caller");
    };
    let descriptor = CloneLimits {
        max_len: 256,
        max_messages: 4,
        max_bytes: 1024,
    };
    let queue = clone_view::create(license, Mark::new(44), descriptor).unwrap();
    let target = info(queue).target;
    let mut source = Source::new("system/test").unwrap();
    source
        .register(Resource {
            name: "queue",
            source: queue,
            maximum: limits(),
            caller: root,
            target: None,
            description: b"native queue",
        })
        .unwrap();
    assert_eq!(related(target), 2);
    assert_eq!(
        source
            .cancel(root, LeaseId::new(70).unwrap())
            .unwrap()
            .state,
        State::Closed
    );
    // A cancellation for a later ID must not erase earlier in-flight requests.
    let requested = request(1, root);
    assert_eq!(
        source.acquire(root, requested.clone()).unwrap().state,
        State::Acquiring
    );
    let delivery = step(&mut source, root, 1, State::Granted);
    let token = PendId::try_from(delivery.delivery.0.as_ref().unwrap().token).unwrap();
    let received = info(token);
    assert_eq!(received.mark, Mark::new(91));
    assert_eq!(received.limits.operations, limits().operations);
    assert_eq!(received.limits.distributable, Distributable::No);
    assert_eq!(
        delivery.delivery.0.as_ref().unwrap().description,
        b"native queue"
    );
    assert_eq!(related(target), 4);
    assert_eq!(source.acquire(root, requested.clone()).unwrap(), delivery);
    assert_eq!(related(target), 4, "replay created another grant");
    let mut different = requested;
    different.binding.slot = system_protocol::Mark::new(92);
    assert_eq!(source.acquire(root, different), Err(Fail::Conflict));
    let unauthorized = request(2, root);
    assert_eq!(
        source.acquire(TaskId::new(999), unauthorized),
        Err(Fail::Denied)
    );
    let mut broad = request(2, root);
    broad.binding.rights.0.distributable = Distributable::Yes;
    assert_eq!(source.acquire(root, broad), Err(Fail::Denied));
    let mut foreign = request(2, root);
    foreign.target = system_protocol::TaskId::new(999);
    assert_eq!(source.acquire(root, foreign), Err(Fail::Denied));
    assert_eq!(related(target), 4);
    source.release(root, LeaseId::new(1).unwrap()).unwrap();
    step(&mut source, root, 1, State::Released);
    assert_eq!(related(target), 2);
    let mut bytes = [0; PendInfo::SIZE];
    assert!(pend::inspect(token, VirtAddr::new(bytes.as_mut_ptr().addr())).is_err());
    for (id, rounds) in [(2, 0), (3, 1), (4, 2)] {
        source.acquire(root, request(id, root)).unwrap();
        for _ in 0..rounds {
            source.advance().unwrap();
        }
        source.cancel(root, LeaseId::new(id).unwrap()).unwrap();
        step(&mut source, root, id, State::Released);
        assert_eq!(related(target), 2, "partial lease retained a grant");
    }
    assert_eq!(
        source.acquire(root, request(70, root)).unwrap().state,
        State::Closed
    );
    // Delayed smaller IDs are accepted while their history remains retained.
    source.acquire(root, request(12, root)).unwrap();
    source.acquire(root, request(11, root)).unwrap();
    source.cancel(root, LeaseId::new(12).unwrap()).unwrap();
    source.cancel(root, LeaseId::new(11).unwrap()).unwrap();
    step(&mut source, root, 12, State::Released);
    step(&mut source, root, 11, State::Released);
    let end = room::clock() + 5_000_000_000;
    while !source.finish().unwrap() {
        assert!(room::clock() < end);
        room::yield_();
    }
    assert_eq!(related(target), 1);
    assert!(source.finish().unwrap());
    // Finishing private lease roots must leave the provider's original alive.
    assert_eq!(info(queue).status, PendStatus::Active);
    pend::return_(queue).unwrap();
    assert_eq!(related(target), 0);
    writeln!(
        Console,
        "PASS supply-native rights=1 scope=1 replay=1 cancel=1 reorder=1 withdrawal=1 cleanup=1"
    )
    .unwrap();
}

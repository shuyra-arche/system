#![allow(dead_code)]
extern crate alloc;
extern crate self as kernel_client;
extern crate self as twine;

use alloc::vec::Vec;
use core::cell::RefCell;
use kernel_api::{
    abi::call::pend::PendFail,
    address::VirtAddr,
    model::{
        identity::{Mark, PendId, TaskId as KernelTaskId},
        modifiable::Modifiable,
        pend::{
            CloneRights, Distributable, Limits, Operations, PendInfo, PendScope, PendStatus,
            Target, ViewTarget,
        },
        view::ViewId,
    },
};
use system_protocol::{
    TaskId,
    operator::{Event, Fail, Kind},
};
use weave::{Codec, Coded, transport};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SendMode {
    Busy,
    Failed,
}

thread_local! {
    static PENDS: RefCell<Vec<PendInfo>> = const { RefCell::new(Vec::new()) };
    static RETURN_FAILURES: RefCell<Vec<(PendId, usize)>> = const { RefCell::new(Vec::new()) };
    static RETURNS: RefCell<Vec<PendId>> = const { RefCell::new(Vec::new()) };
    static SEND_MODES: RefCell<Vec<(PendId, SendMode)>> = const { RefCell::new(Vec::new()) };
    static ATTEMPTED: RefCell<Vec<PendId>> = const { RefCell::new(Vec::new()) };
    static SENT: RefCell<Vec<(PendId, Event)>> = const { RefCell::new(Vec::new()) };
}

pub mod call {
    pub mod pend {
        use super::super::*;

        pub fn inspect(token: PendId, address: VirtAddr) -> Result<(), erra::Error<PendFail>> {
            let found = PENDS.with(|pends| {
                pends
                    .borrow()
                    .iter()
                    .copied()
                    .find(|info| info.token == token)
            });
            let Some(info) = found else {
                return Err(erra::Error::new("fake inspect", PendFail::Denied));
            };
            // The real ABI writes these words to the caller-provided buffer. This
            // test double models only that Kernel boundary; Watchers remains real.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    info.words().as_ptr(),
                    address.get() as *mut usize,
                    PendInfo::WORDS,
                );
            }
            Ok(())
        }

        pub fn return_(token: PendId) -> Result<(), erra::Error<PendFail>> {
            RETURNS.with(|returns| returns.borrow_mut().push(token));
            let fail = RETURN_FAILURES.with(|failures| {
                let mut failures = failures.borrow_mut();
                let Some((_, remaining)) = failures.iter_mut().find(|(id, _)| *id == token) else {
                    return false;
                };
                if *remaining == 0 {
                    false
                } else {
                    *remaining -= 1;
                    true
                }
            });
            if fail {
                return Err(erra::Error::new("fake return", PendFail::Busy));
            }
            PENDS.with(|pends| pends.borrow_mut().retain(|info| info.token != token));
            Ok(())
        }
    }
}

pub mod view {
    pub mod clone {
        use super::super::*;

        pub struct Sender(PendId);
        impl Sender {
            pub fn new(token: PendId) -> Self {
                Self(token)
            }
        }

        impl transport::FrameSender for Sender {
            type Error = erra::Error<kernel_api::abi::call::clone_view::CloneFail>;

            fn send(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
                ATTEMPTED.with(|attempted| attempted.borrow_mut().push(self.0));
                let event = Event::decode_exact(bytes)
                    .expect("real Event codec must produce a complete frame");
                let mode = SEND_MODES.with(|modes| {
                    modes
                        .borrow()
                        .iter()
                        .find(|(token, _)| *token == self.0)
                        .map(|(_, mode)| *mode)
                });
                match mode {
                    Some(SendMode::Busy) => Err(erra::Error::new(
                        "fake send",
                        kernel_api::abi::call::clone_view::CloneFail::Busy,
                    )),
                    Some(SendMode::Failed) => Err(erra::Error::new(
                        "fake send",
                        kernel_api::abi::call::clone_view::CloneFail::Dead,
                    )),
                    None => {
                        SENT.with(|sent| sent.borrow_mut().push((self.0, event)));
                        Ok(())
                    }
                }
            }
        }
    }
}

#[path = "../../../src/system/operator/watch.rs"]
mod watch;

fn task(value: u64) -> TaskId {
    TaskId::new(value)
}

fn kernel_task(value: usize) -> KernelTaskId {
    KernelTaskId::new(value)
}

fn token(value: usize) -> PendId {
    PendId::mint(value)
}

fn info(token: PendId, caller: TaskId) -> PendInfo {
    PendInfo {
        token,
        target: Target::View(ViewTarget::Clone(ViewId::new(1))),
        limits: Limits {
            operations: Operations::Clone(CloneRights::PUSH | CloneRights::PEEK),
            modifiable: Modifiable::Yes,
            distributable: Distributable::Yes,
            scope: PendScope::Object,
        },
        owner: KernelTaskId::new(caller.get() as usize),
        lender: KernelTaskId::new(caller.get() as usize),
        mark: Mark::NONE,
        status: PendStatus::Active,
    }
}

fn path(value: &str) -> system_protocol::Path {
    system_protocol::Path::new(value).unwrap()
}

fn event(value: &str) -> Event {
    Event {
        sequence: 1,
        kind: Coded(Kind::Landed),
        path: path(value),
        entry: system_protocol::EntryId::new(1),
        owner: task(1),
    }
}

fn reset(infos: impl IntoIterator<Item = PendInfo>) {
    PENDS.with(|pends| *pends.borrow_mut() = infos.into_iter().collect());
    RETURN_FAILURES.with(|failures| failures.borrow_mut().clear());
    RETURNS.with(|returns| returns.borrow_mut().clear());
    SEND_MODES.with(|modes| modes.borrow_mut().clear());
    ATTEMPTED.with(|attempted| attempted.borrow_mut().clear());
    SENT.with(|sent| sent.borrow_mut().clear());
}

#[test]
fn invalid_kernel_imports_do_not_replace_an_existing_live_subscription() {
    let caller = task(1);
    let valid = info(token(10), caller);
    let mut watchers = watch::Watchers::new().unwrap();
    reset([valid]);
    watchers.join(caller, valid.token, path("svc/a")).unwrap();

    let mut malformed = Vec::new();
    let mut wrong_lender = info(token(11), caller);
    wrong_lender.lender = kernel_task(2);
    malformed.push(wrong_lender);
    let mut wrong_owner = info(token(12), caller);
    wrong_owner.owner = kernel_task(2);
    malformed.push(wrong_owner);
    let mut suspended = info(token(13), caller);
    suspended.status = PendStatus::Suspended;
    malformed.push(suspended);
    let mut no_push = info(token(14), caller);
    no_push.limits.operations = Operations::Clone(CloneRights::PEEK);
    malformed.push(no_push);
    let mut immutable = info(token(15), caller);
    immutable.limits.modifiable = Modifiable::No;
    malformed.push(immutable);

    for invalid in malformed {
        PENDS.with(|pends| pends.borrow_mut().push(invalid));
        assert!(matches!(
            watchers.join(caller, invalid.token, path("svc/forged")),
            Err(Fail::Denied)
        ));
        watchers.sweep();
        assert!(watchers.contains(valid.token));
        assert!(!watchers.contains(invalid.token));
    }

    watchers.publish(&event("svc/a/item"));
    assert_eq!(
        SENT.with(|sent| sent.borrow().iter().map(|(id, _)| *id).collect::<Vec<_>>()),
        vec![valid.token]
    );
}

#[test]
fn sweep_retires_only_the_dead_subscription_and_keeps_other_callers() {
    let first = info(token(20), task(1));
    let second = info(token(21), task(2));
    reset([first, second]);
    let mut watchers = watch::Watchers::new().unwrap();
    watchers.join(task(1), first.token, path("")).unwrap();
    watchers.join(task(2), second.token, path("")).unwrap();
    PENDS.with(|pends| {
        pends
            .borrow_mut()
            .iter_mut()
            .find(|info| info.token == first.token)
            .unwrap()
            .status = PendStatus::Dead;
    });

    watchers.sweep();

    assert!(!watchers.contains(first.token));
    assert!(watchers.contains(second.token));
    assert_eq!(
        RETURNS.with(|returns| returns.borrow().clone()),
        vec![first.token]
    );
    watchers.publish(&event("anything"));
    assert_eq!(
        SENT.with(|sent| sent.borrow().iter().map(|(id, _)| *id).collect::<Vec<_>>()),
        vec![second.token]
    );
}

#[test]
fn failed_return_keeps_closing_owner_and_allows_same_path_rejoin() {
    let caller = task(3);
    let old = info(token(30), caller);
    let replacement = info(token(31), caller);
    reset([old, replacement]);
    let mut watchers = watch::Watchers::new().unwrap();
    watchers.join(caller, old.token, path("svc/a")).unwrap();
    SEND_MODES.with(|modes| modes.borrow_mut().push((old.token, SendMode::Failed)));
    watchers.publish(&event("svc/a/item"));
    RETURN_FAILURES.with(|failures| failures.borrow_mut().push((old.token, 1)));

    watchers.sweep();
    assert!(watchers.contains(old.token));
    watchers
        .join(caller, replacement.token, path("svc/a"))
        .unwrap();
    watchers.sweep();
    assert!(!watchers.contains(old.token));
    assert!(watchers.contains(replacement.token));
    assert!(RETURNS.with(|returns| {
        returns
            .borrow()
            .iter()
            .filter(|id| **id == old.token)
            .count()
            >= 2
    }));
}

#[test]
fn busy_send_keeps_subscription_and_prefix_requires_a_path_boundary() {
    let subscriber = info(token(40), task(4));
    reset([subscriber]);
    let mut watchers = watch::Watchers::new().unwrap();
    watchers
        .join(task(4), subscriber.token, path("svc/a"))
        .unwrap();
    SEND_MODES.with(|modes| modes.borrow_mut().push((subscriber.token, SendMode::Busy)));

    watchers.publish(&event("svc/a-b/item"));
    assert!(ATTEMPTED.with(|attempted| attempted.borrow().is_empty()));
    assert!(SENT.with(|sent| sent.borrow().is_empty()));
    assert!(watchers.contains(subscriber.token));
    watchers.publish(&event("svc/a/item"));
    assert_eq!(
        ATTEMPTED.with(|attempted| attempted.borrow().clone()),
        vec![subscriber.token]
    );
    assert!(watchers.contains(subscriber.token));

    SEND_MODES.with(|modes| {
        modes
            .borrow_mut()
            .iter_mut()
            .find(|(id, _)| *id == subscriber.token)
            .unwrap()
            .1 = SendMode::Failed;
    });
    watchers.publish(&event("svc/a/item"));
    watchers.sweep();
    assert!(!watchers.contains(subscriber.token));
}

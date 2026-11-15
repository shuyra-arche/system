//! Scripted delivery/receipt tests. These do not execute native traps.
extern crate self as kernel_client;
use kernel_api::abi::memory::{Call, Request};
use std::cell::{Cell, RefCell};

#[allow(dead_code)]
#[path = "../../src/runtime/memory.rs"]
mod owner;

type Delivery = Result<memory::Response, memory::Error>;
type Transport = Box<dyn FnMut(Request) -> Delivery>;
thread_local! {
    static SCRIPT: RefCell<Option<Transport>> = RefCell::new(None);
    static CLOCK: Cell<u64> = const { Cell::new(0) };
}
pub mod call {
    pub mod room {
        pub fn clock() -> u64 {
            crate::CLOCK.with(|clock| clock.get())
        }
        pub fn yield_() {
            crate::CLOCK.with(|clock| clock.set(clock.get() + 1));
        }
    }
}
pub mod memory {
    use kernel_api::abi::memory::{Failure, Observation, Reply};
    #[derive(Clone, Copy, Debug)]
    pub enum Response {
        Reply(Reply),
        Observation(Observation),
    }
    #[derive(Clone, Copy, Debug)]
    pub enum Error {
        Delivery(Failure),
        Protocol,
    }
    pub mod flat {
        pub fn invoke(call: kernel_api::abi::memory::flat::Call) -> crate::Delivery {
            crate::invoke(call.into())
        }
    }
    pub mod nest {
        pub fn invoke(call: kernel_api::abi::memory::nest::Call) -> crate::Delivery {
            crate::invoke(call.into())
        }
    }
}
fn invoke(call: Call) -> Delivery {
    SCRIPT.with_borrow_mut(|script| script.as_mut().expect("unexpected call")(call.request()))
}

#[cfg(test)]
mod contracts {
    use super::*;
    use kernel_api::{
        abi::memory::{Action, Failure, Observation, Opcode, Outcome, Reply, Snapshot, Value},
        address::VirtAddr,
        model::{
            cell::CellLevel,
            identity::PendId,
            nest::{
                MemoPosition, NestId, ReservationId, ReservationPhase, ReservationStatus,
                ReservationVersion,
            },
        },
    };
    use std::{cell::RefCell, rc::Rc};

    fn memo() -> MemoPosition {
        MemoPosition::new(
            PendId::mint(7),
            NestId::new(2).unwrap(),
            ReservationId::from_bytes(&9u64.to_le_bytes()).unwrap(),
            0,
        )
        .unwrap()
    }
    fn lifecycle(version: u64, phase: ReservationPhase) -> Value {
        Value::ReservationLifecycle {
            nest: memo().nest(),
            reservation: memo().reservation(),
            version: ReservationVersion::new(version).unwrap(),
            phase,
        }
    }
    fn observation(version: u64, phase: ReservationPhase) -> Delivery {
        Ok(memory::Response::Observation(
            Observation::new(Snapshot::Reservation(ReservationStatus {
                nest: memo().nest(),
                reservation: memo().reservation(),
                version: ReservationVersion::new(version).unwrap(),
                phase,
                offset: 0,
                count: 1,
                address: VirtAddr::new(0x4000),
                level: CellLevel::BASE,
                access: 6,
            }))
            .unwrap(),
        ))
    }
    fn reply(request: Request, outcome: Outcome) -> Delivery {
        let id = match request.action() {
            Action::InspectResult { request, .. } => request,
            _ => request.id().unwrap(),
        };
        Ok(memory::Response::Reply(
            Reply::new(id, Opcode::Release, outcome).unwrap(),
        ))
    }
    fn run(script: impl FnMut(Request) -> Delivery + 'static) -> Result<(), Failure> {
        CLOCK.with(|clock| clock.set(0));
        SCRIPT.with_borrow_mut(|slot| *slot = Some(Box::new(script)));
        owner::Memory::new(|note| panic!("{note}")).release(memo())
    }

    #[test]
    fn pending_and_lost_reply_keep_identity_terminal_busy_gets_explicit_continuation() {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let seen = requests.clone();
        let failed = Rc::new(RefCell::new(None));
        let retained = failed.clone();
        run(move |request| {
            let mut seen = seen.borrow_mut();
            seen.push(request);
            match seen.len() {
                1 => observation(3, ReservationPhase::Open),
                2 => reply(
                    request,
                    Outcome::Pending(lifecycle(4, ReservationPhase::Releasing)),
                ),
                3 => Err(memory::Error::Delivery(Failure::Busy)),
                4 => {
                    let response = reply(
                        request,
                        Outcome::Failed {
                            failure: Failure::Busy,
                            retained: Some(lifecycle(4, ReservationPhase::Releasing)),
                        },
                    );
                    if let Ok(memory::Response::Reply(value)) = response {
                        *retained.borrow_mut() = Some(value);
                    }
                    response
                }
                5 => observation(5, ReservationPhase::Releasing),
                6 => reply(
                    request,
                    Outcome::Committed(lifecycle(6, ReservationPhase::Released)),
                ),
                _ => panic!("replayed effect"),
            }
        })
        .unwrap();
        let seen = requests.borrow();
        assert_eq!(seen.len(), 6);
        let original = seen[1].id().unwrap();
        for request in &seen[2..4] {
            assert!(
                matches!(request.action(), Action::InspectResult { request, .. } if request == original)
            );
        }
        assert!(seen[5].id().unwrap() > original);
        assert!(
            matches!(seen[5].action(), Action::Release { expected, .. } if expected.get() == 5)
        );
        assert!(matches!(
            failed.borrow().unwrap().outcome(),
            Outcome::Failed {
                failure: Failure::Busy,
                ..
            }
        ));
    }

    #[test]
    fn initial_delivery_failure_queries_the_accepted_release_instead_of_replaying() {
        let mut step = 0;
        let mut id = None;
        run(move |request| {
            step += 1;
            match step {
                1 => observation(3, ReservationPhase::Open),
                2 => { id = request.id(); Err(memory::Error::Protocol) }
                3 => {
                    assert!(matches!(request.action(), Action::InspectResult { request, .. } if Some(request) == id));
                    reply(request, Outcome::Committed(lifecycle(4, ReservationPhase::Released)))
                }
                _ => panic!("replayed lost delivery"),
            }
        }).unwrap();
    }

    #[test]
    fn rejection_does_not_authorize_release_or_return() {
        let mut step = 0;
        assert_eq!(
            run(move |request| {
                step += 1;
                match step {
                    1 => observation(3, ReservationPhase::Open),
                    2 => reply(request, Outcome::Rejected(Failure::Denied)),
                    _ => panic!("continued denied work"),
                }
            }),
            Err(Failure::Denied)
        );
    }

    #[test]
    fn retained_version_cannot_be_replaced_by_an_older_snapshot() {
        let mut step = 0;
        let error = std::panic::catch_unwind(|| {
            run(move |request| {
                step += 1;
                match step {
                    1 => observation(3, ReservationPhase::Open),
                    2 => reply(
                        request,
                        Outcome::Failed {
                            failure: Failure::Busy,
                            retained: Some(lifecycle(5, ReservationPhase::Releasing)),
                        },
                    ),
                    3 => observation(4, ReservationPhase::Releasing),
                    _ => panic!("used a regressed reservation version"),
                }
            })
        })
        .expect_err("regressed snapshot must abort");
        assert_eq!(
            error.downcast_ref::<String>().unwrap(),
            "reservation observation regressed"
        );
    }

    #[test]
    fn pending_deadline_never_allocates_a_new_mutation_identity() {
        let mut step = 0;
        let mut original = None;
        let error = std::panic::catch_unwind(|| run(move |request| {
            step += 1;
            match step {
                1 => observation(3, ReservationPhase::Open),
                2 => {
                    original = request.id();
                    reply(request, Outcome::Pending(lifecycle(4, ReservationPhase::Releasing)))
                }
                3 => {
                    assert!(matches!(request.action(), Action::InspectResult { request, .. } if Some(request) == original));
                    CLOCK.with(|clock| clock.set(5_000_000_000));
                    reply(request, Outcome::Pending(lifecycle(4, ReservationPhase::Releasing)))
                }
                _ => panic!("replayed a pending operation"),
            }
        })).expect_err("pending deadline must abort");
        assert_eq!(
            error.downcast_ref::<String>().unwrap(),
            "memory result timed out"
        );
    }

    #[test]
    fn incompatible_retained_results_are_not_continuations() {
        let Value::ReservationLifecycle {
            nest,
            version,
            phase,
            ..
        } = lifecycle(4, ReservationPhase::Releasing)
        else {
            unreachable!()
        };
        let foreign = Value::ReservationLifecycle {
            nest,
            reservation: ReservationId::from_bytes(&99u64.to_le_bytes()).unwrap(),
            version,
            phase,
        };
        for value in [
            foreign,
            lifecycle(2, ReservationPhase::Releasing),
            lifecycle(4, ReservationPhase::Failed),
        ] {
            let mut step = 0;
            let result = std::panic::catch_unwind(|| {
                run(move |request| {
                    step += 1;
                    match step {
                        1 => observation(3, ReservationPhase::Open),
                        2 => reply(
                            request,
                            Outcome::Failed {
                                failure: Failure::Busy,
                                retained: Some(value),
                            },
                        ),
                        _ => panic!("unsafe continuation"),
                    }
                })
            });
            let error = result.expect_err("retained failure must abort");
            let message = error
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| error.downcast_ref::<&str>().copied())
                .unwrap();
            assert_eq!(message, "reservation retirement unsettled");
        }
    }
}

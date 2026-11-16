//! Scripted host regressions for the Control Instance retirement owner.
//!
//! The production `Gate` is compiled from its source path. This facade scripts
//! service discovery, pending calls, cleanup and root release; it verifies the
//! gate's ordering and retry policy, not native traps or provider execution.
extern crate alloc;

use std::{cell::RefCell, collections::VecDeque, marker::PhantomData};

extern crate self as kernel_client;
extern crate self as system_client;

#[derive(Clone)]
pub struct Context;
pub struct Cleanup;
#[derive(Debug)]
pub struct TransportError;
pub struct Pending<T>(PhantomData<T>);
pub struct Response<T> {
    pub value: T,
}

#[derive(Clone, Copy)]
pub enum Poll {
    Pending,
    Settled(u64),
    Unsettled(u64),
    Failure,
    TransportFailure,
}

#[derive(Default)]
pub struct Effects {
    pub calls: Vec<String>,
    pub begins: VecDeque<Result<(), TransportError>>,
    pub polls: VecDeque<Poll>,
    pub cleanups: VecDeque<Result<(), TransportError>>,
    pub releases: VecDeque<bool>,
    pub resource_failures: VecDeque<bool>,
}

pub mod call {
    pub mod room {
        pub fn clock() -> u64 {
            0
        }
    }
}

thread_local! {
    static EFFECTS: RefCell<Effects> = RefCell::new(Effects::default());
}

pub fn reset(effects: Effects) {
    EFFECTS.with(|slot| *slot.borrow_mut() = effects);
}

impl Cleanup {
    pub fn finish(&mut self) -> Result<(), TransportError> {
        EFFECTS.with(|slot| {
            let mut effects = slot.borrow_mut();
            effects.calls.push("cleanup".into());
            effects.cleanups.pop_front().unwrap_or(Ok(()))
        })
    }
}

impl<T> Pending<T> {
    pub fn poll(
        &mut self,
    ) -> Result<
        Option<
            Response<
                system_protocol::value::Outcome<
                    system_protocol::retirement::Receipt,
                    system_protocol::retirement::Fail,
                >,
            >,
        >,
        TransportError,
    > {
        EFFECTS.with(|slot| {
            let mut effects = slot.borrow_mut();
            effects.calls.push("poll".into());
            match effects.polls.pop_front().unwrap_or(Poll::Pending) {
                Poll::Pending => Ok(None),
                Poll::Settled(task) => Ok(Some(Response {
                    value: system_protocol::value::Outcome::Success(
                        system_protocol::retirement::Receipt {
                            task: system_protocol::TaskId::new(task),
                            state: system_protocol::retirement::State::Settled,
                        },
                    ),
                })),
                Poll::Unsettled(task) => Ok(Some(Response {
                    value: system_protocol::value::Outcome::Success(
                        system_protocol::retirement::Receipt {
                            task: system_protocol::TaskId::new(task),
                            state: system_protocol::retirement::State::Pending,
                        },
                    ),
                })),
                Poll::Failure => Ok(Some(Response {
                    value: system_protocol::value::Outcome::Failure(
                        system_protocol::retirement::Fail::Failed,
                    ),
                })),
                Poll::TransportFailure => Err(TransportError),
            }
        })
    }

    pub fn into_cleanup(self) -> Cleanup {
        Cleanup
    }
}

pub mod retirement {
    use super::{Cleanup, EFFECTS, Pending};
    use kernel_api::model::identity::{PendId, TaskId};
    use kernel_api::model::wait::Wait;
    use system_protocol::{operator as op, retirement as api};

    pub struct Client;
    pub struct BeginError {
        pub cleanup: Cleanup,
    }

    impl Client {
        pub fn bind(
            _: PendId,
            _: TaskId,
            _: PendId,
            _: super::Context,
        ) -> Result<Self, super::TransportError> {
            Ok(Self)
        }

        pub fn begin<T>(&mut self, target: api::Target, _: Wait) -> Result<Pending<T>, BeginError> {
            EFFECTS.with(|slot| {
                let mut effects = slot.borrow_mut();
                effects.calls.push(format!("begin:{}", target.task.get()));
                match effects.begins.pop_front().unwrap_or(Ok(())) {
                    Ok(()) => Ok(Pending(std::marker::PhantomData)),
                    Err(_) => Err(BeginError { cleanup: Cleanup }),
                }
            })
        }
    }

    pub fn anchor() -> op::Entry {
        op::Entry {
            operator: system_protocol::TaskId::new(4),
            target: op::EntryTarget::Root,
        }
    }
}

mod system {
    pub mod app {
        pub mod namespace {
            use kernel_api::model::identity::TaskId;
            use kernel_api::model::pend::{Distributable, Limits, Operations, PendScope};
            use kernel_api::model::{modifiable::Modifiable, sign::SignRights};
            use system_protocol::{operator as op, value::Rights};

            pub struct Layout;
            impl Layout {
                pub fn resource(
                    &self,
                    _: &super::services::Services,
                    _: op::Entry,
                    _: &str,
                ) -> Result<op::Resource, op::Fail> {
                    let fail = crate::EFFECTS.with(|slot| {
                        slot.borrow_mut()
                            .resource_failures
                            .pop_front()
                            .unwrap_or(false)
                    });
                    if fail {
                        return Err(op::Fail::Unknown);
                    }
                    Ok(op::Resource {
                        binding: op::Binding {
                            entry: op::Entry {
                                operator: system_protocol::TaskId::new(4),
                                target: op::EntryTarget::Root,
                            },
                            revision: 1,
                        },
                        token: system_protocol::PendId::new(90),
                        rights: Rights(Limits {
                            operations: Operations::Sign(SignRights::TRIGGER),
                            modifiable: Modifiable::No,
                            distributable: Distributable::No,
                            scope: PendScope::Object,
                        }),
                        permit: op::Permit::Public,
                    })
                }
            }

            pub fn _task(_: TaskId) {}
        }

        pub mod services {
            use kernel_api::model::identity::{PendId, TaskId};

            pub struct Services {
                pub license: PendId,
                pub context: crate::Context,
            }

            pub struct Info {
                pub owner: TaskId,
            }

            pub fn inspect(_: PendId) -> Info {
                Info {
                    owner: TaskId::try_from(system_protocol::TaskId::new(4)).unwrap(),
                }
            }

            pub fn release(_: PendId) -> bool {
                crate::EFFECTS.with(|slot| {
                    let mut effects = slot.borrow_mut();
                    effects.calls.push("release".into());
                    effects.releases.pop_front().unwrap_or(true)
                })
            }
        }
    }
}

#[path = "../../src/system/control/instance/retirement.rs"]
mod production_gate;

mod gate;

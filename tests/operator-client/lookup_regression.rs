use ipc_exchange_fixture::{
    raw,
    session::{Endpoint, Session},
    Contract, Number, PieToken, TaskId, Wait,
};
use std::cell::Cell;
use system_api::operator::Fail;

#[path = "../../client/src/operator/lookup.rs"]
mod lookup;

struct ContractNum;

impl Contract for ContractNum {
    type Request = Number;
    type Response = Number;
}

const UNKNOWN: u8 = 9;
const DENIED: u8 = 4;

fn session() -> Session {
    Session::from_raw(
        Endpoint {
            sender: Some(PieToken(1)),
            receiver: PieToken(2),
        },
        PieToken(3),
        TaskId(42),
    )
    .unwrap()
}

fn call(
    session: &Session,
    wait: Wait,
    retry_missing: bool,
) -> Result<u8, lookup::Failure> {
    let value = session
        .call::<ContractNum>(Number(4), wait)
        .map_err(|_| lookup::Failure::Stop(Fail::Unjudged))?;
    match value {
        UNKNOWN if retry_missing => Err(lookup::Failure::Missing),
        UNKNOWN => Err(lookup::Failure::Stop(Fail::Unknown)),
        DENIED => Err(lookup::Failure::Stop(Fail::Denied)),
        value => Ok(value),
    }
}

#[test]
fn finite_reply_timeout_then_existing_path_lookup_never_retries_closed_session() {
    raw::reset();
    let s = Session::from_raw(
        Endpoint {
            sender: Some(PieToken(1)),
            receiver: PieToken(2),
        },
        PieToken(3),
        TaskId(42),
    )
    .unwrap();
    let call = |wait| {
        s.call::<ContractNum>(Number(4), wait)
            .map_err(|_| lookup::Failure::Stop(Fail::Unknown))
    };
    assert!(lookup::until(
        || call(Wait::AtMost(5)),
        |_| panic!("transport timeout must not wait for publication")
    )
    .is_err());
    // A real peer would answer this already published path; the old Session remains closed.
    raw::reply_on_next_push(vec![15], TaskId(42));
    assert!(lookup::until(
        || call(Wait::Forever),
        |_| panic!("closed Session must return immediately")
    )
    .is_err());
    assert_eq!(raw::pushes(), 1);
    assert!(!raw::alive(PieToken(2)));
}

#[test]
fn explicit_missing_path_retries_with_backoff_until_published() {
    let mut tries = 0;
    let mut pauses = Vec::new();
    let id = lookup::until(
        || {
            tries += 1;
            if tries < 4 {
                Err(lookup::Failure::Missing)
            } else {
                Ok(42)
            }
        },
        |ms| {
            pauses.push(ms);
            true
        },
    )
    .unwrap();
    assert_eq!(id, 42);
    assert_eq!(pauses, vec![10, 20, 40]);
}

#[test]
fn expired_after_explicit_unknown_returns_unknown_without_second_call() {
    raw::reset();
    let s = session();
    raw::reply_on_next_push(vec![UNKNOWN], TaskId(42));
    let samples = Cell::new(0);
    let calls = Cell::new(0);
    let retries = Cell::new(0);
    let result: Result<u8, Fail> = lookup::until_after_unknown(
        true,
        || {
            samples.set(samples.get() + 1);
            if calls.get() == 0 {
                (Wait::AtMost(5), false)
            } else {
                (Wait::POLL, true)
            }
        },
        |remaining| {
            calls.set(calls.get() + 1);
            call(&s, remaining, true)
        },
        |_| {
            retries.set(retries.get() + 1);
            retries.get() == 1
        },
    );
    assert_eq!(result, Err(Fail::Unknown));
    assert_eq!(samples.get(), 2, "one budget sample per attempted retry");
    assert_eq!(calls.get(), 1, "no second request may be sent at POLL");
    assert_eq!(retries.get(), 2);
    assert_eq!(raw::pushes(), 1);
    assert!(raw::alive(PieToken(2)));
}

#[test]
fn captured_positive_budget_is_exact_session_value_without_second_read() {
    raw::reset();
    let s = session();
    let samples = Cell::new(0);
    let calls = Cell::new(0);
    let captured = Cell::new(Wait::POLL);
    let result: Result<u8, Fail> = lookup::until_after_unknown(
        true,
        || {
            let sample = samples.get();
            samples.set(sample + 1);
            if sample == 0 {
                (Wait::AtMost(1), false)
            } else {
                (Wait::POLL, true)
            }
        },
        |remaining| {
            calls.set(calls.get() + 1);
            captured.set(remaining);
            call(&s, remaining, true)
        },
        |_| panic!("transport failure must stop the retry loop"),
    );
    assert_eq!(result, Err(Fail::Unjudged));
    assert_eq!(samples.get(), 1);
    assert_eq!(calls.get(), 1);
    assert_eq!(captured.get(), Wait::AtMost(1));
    assert_eq!(raw::last_pull(), Some((1, Wait::AtMost(1))));
    assert_eq!(raw::pushes(), 1);
}

#[test]
fn first_poll_without_unknown_keeps_transport_unjudged() {
    raw::reset();
    let s = session();
    let samples = Cell::new(0);
    let calls = Cell::new(0);
    let result: Result<u8, Fail> = lookup::until_after_unknown(
        true,
        || {
            samples.set(samples.get() + 1);
            (Wait::POLL, true)
        },
        |remaining| {
            calls.set(calls.get() + 1);
            call(&s, remaining, true)
        },
        |_| panic!("first POLL transport failure must not retry"),
    );
    assert_eq!(result, Err(Fail::Unjudged));
    assert_eq!(samples.get(), 1);
    assert_eq!(calls.get(), 1);
}

#[test]
fn positive_budget_transport_failure_after_unknown_stays_unjudged() {
    raw::reset();
    let s = session();
    raw::reply_on_next_push(vec![UNKNOWN], TaskId(42));
    let samples = Cell::new(0);
    let calls = Cell::new(0);
    let result: Result<u8, Fail> = lookup::until_after_unknown(
        true,
        || {
            samples.set(samples.get() + 1);
            if calls.get() == 0 {
                (Wait::AtMost(5), false)
            } else {
                (Wait::AtMost(1), false)
            }
        },
        |remaining| {
            calls.set(calls.get() + 1);
            call(&s, remaining, true)
        },
        |_| true,
    );
    assert_eq!(result, Err(Fail::Unjudged));
    assert_eq!(samples.get(), 2);
    assert_eq!(calls.get(), 2);
}

#[test]
fn resolve_first_unknown_remains_immediate_unknown_without_retry() {
    raw::reset();
    let s = session();
    raw::reply_on_next_push(vec![UNKNOWN], TaskId(42));
    let samples = Cell::new(0);
    let calls = Cell::new(0);
    let result: Result<u8, Fail> = lookup::until_after_unknown(
        false,
        || {
            samples.set(samples.get() + 1);
            (Wait::AtMost(5), false)
        },
        |remaining| {
            calls.set(calls.get() + 1);
            call(&s, remaining, false)
        },
        |_| panic!("resolve UNKNOWN must not retry"),
    );
    assert_eq!(result, Err(Fail::Unknown));
    assert_eq!(samples.get(), 1);
    assert_eq!(calls.get(), 1);
    assert_eq!(raw::pushes(), 1);
}

#[test]
fn denied_response_keeps_denied() {
    raw::reset();
    let s = session();
    raw::reply_on_next_push(vec![DENIED], TaskId(42));
    let samples = Cell::new(0);
    let calls = Cell::new(0);
    let result: Result<u8, Fail> = lookup::until_after_unknown(
        true,
        || {
            samples.set(samples.get() + 1);
            (Wait::AtMost(5), false)
        },
        |remaining| {
            calls.set(calls.get() + 1);
            call(&s, remaining, true)
        },
        |_| panic!("Denied must not retry"),
    );
    assert_eq!(result, Err(Fail::Denied));
    assert_eq!(samples.get(), 1);
    assert_eq!(calls.get(), 1);
}

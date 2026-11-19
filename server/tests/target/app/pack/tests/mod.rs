#[path = "../observations.rs"]
mod observations;

use unit_contract::event::{self, Cause, Event, Exit};

fn frame(events: &[Event<&str>]) -> Vec<u8> {
    events.iter().fold(Vec::new(), |mut bytes, event| {
        bytes.extend_from_slice(event::encode(event).unwrap().as_bytes());
        bytes
    })
}

fn complete(ready: bool) -> Vec<Event<&'static str>> {
    let mut events = vec![
        Event::Started { system: 1 },
        Event::Bound {
            unit: "worker",
            task: 10,
            team: 20,
        },
    ];
    if ready {
        events.push(Event::Ready { task: 10 });
    }
    events.extend([
        Event::Exited {
            task: 10,
            exit: Exit {
                cause: Cause::Reap,
                reason: 0,
            },
        },
        Event::Retired { task: 10 },
        Event::Complete {
            system: 1,
            reason: 0,
        },
    ]);
    events
}

fn rejects(events: &[Event<&str>]) {
    assert!(observations::decode(&frame(events)).is_err(), "{events:?}");
}

#[test]
fn accepts_interleaved_tasks_and_partial_creation_failure_without_ready() {
    let events = [
        Event::Started { system: 1 },
        Event::Bound {
            unit: "controller",
            task: 10,
            team: 20,
        },
        Event::Ready { task: 10 },
        Event::Bound {
            unit: "driver",
            task: 11,
            team: 21,
        },
        Event::Exited {
            task: 10,
            exit: Exit {
                cause: Cause::Reap,
                reason: 0,
            },
        },
        Event::Retired { task: 10 },
        Event::Exited {
            task: 11,
            exit: Exit {
                cause: Cause::Slay,
                reason: 4,
            },
        },
        Event::Retired { task: 11 },
        Event::Complete {
            system: 1,
            reason: 0,
        },
    ];
    assert!(observations::decode(&frame(&events)).is_ok());
    assert!(observations::decode(&frame(&complete(false))).is_ok());
}

#[test]
fn rejects_missing_truncated_or_misplaced_stream_boundaries() {
    assert!(observations::decode(b"no typed events\n").is_err());
    rejects(&[Event::Complete {
        system: 1,
        reason: 0,
    }]);

    let mut missing_complete = complete(true);
    missing_complete.pop();
    rejects(&missing_complete);

    let mut complete_not_last = complete(true);
    complete_not_last.push(Event::Bound {
        unit: "later",
        task: 11,
        team: 21,
    });
    rejects(&complete_not_last);

    let mut mismatched_complete = complete(true);
    *mismatched_complete.last_mut().unwrap() = Event::Complete {
        system: 2,
        reason: 0,
    };
    rejects(&mismatched_complete);

    let mut truncated = event::encode(&Event::Started { system: 1 })
        .unwrap()
        .into_bytes();
    truncated.pop();
    assert!(observations::decode(&truncated).is_err());
}

#[test]
fn rejects_duplicate_start_complete_and_task_events() {
    let mut duplicate_started = complete(true);
    duplicate_started.insert(1, Event::Started { system: 1 });
    rejects(&duplicate_started);

    let mut duplicate_complete = complete(true);
    let last = duplicate_complete.last().unwrap().clone();
    duplicate_complete.insert(duplicate_complete.len() - 1, last);
    rejects(&duplicate_complete);

    let mut duplicate_bound = complete(true);
    duplicate_bound.insert(2, duplicate_bound[1].clone());
    rejects(&duplicate_bound);

    for event in [
        Event::Ready { task: 10 },
        Event::Exited {
            task: 10,
            exit: Exit {
                cause: Cause::Reap,
                reason: 0,
            },
        },
        Event::Retired { task: 10 },
    ] {
        let mut duplicated = complete(true);
        duplicated.insert(duplicated.len() - 1, event);
        rejects(&duplicated);
    }
}

#[test]
fn rejects_invalid_task_identity_association_and_order() {
    for (task, team) in [(0, 20), (10, 0)] {
        let mut events = complete(true);
        events[1] = Event::Bound {
            unit: "worker",
            task,
            team,
        };
        rejects(&events);
    }

    let mut repeated_task = complete(true);
    repeated_task.insert(
        2,
        Event::Bound {
            unit: "restart",
            task: 10,
            team: 21,
        },
    );
    rejects(&repeated_task);

    let mut repeated_team = complete(true);
    repeated_team.insert(
        2,
        Event::Bound {
            unit: "other",
            task: 11,
            team: 20,
        },
    );
    rejects(&repeated_team);

    for event in [
        Event::Ready { task: 99 },
        Event::Exited {
            task: 99,
            exit: Exit {
                cause: Cause::Reap,
                reason: 0,
            },
        },
        Event::Retired { task: 99 },
    ] {
        let mut events = complete(true);
        events.insert(events.len() - 1, event);
        rejects(&events);
    }

    let mut ready_after_exit = complete(true);
    ready_after_exit.swap(2, 3);
    rejects(&ready_after_exit);

    let mut retired_before_exit = complete(true);
    retired_before_exit.swap(3, 4);
    rejects(&retired_before_exit);
}

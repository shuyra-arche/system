use std::collections::HashMap;
use unit_contract::event::{self, Event};

#[derive(Default)]
struct TaskEvents {
    bound: usize,
    ready: Option<usize>,
    exited: Option<usize>,
    retired: Option<usize>,
}

/// Decode and validate the typed event stream embedded in a serial log.
/// Kernel console text before the marker may be truncated; once the marker is
/// present the event frame itself must be complete and valid.
pub fn decode(log: &[u8]) -> Result<Vec<Event<String>>, String> {
    let mut events = Vec::new();
    for line in log.split_inclusive(|byte| *byte == b'\n') {
        let Some(offset) = line
            .windows(event::PREFIX.len())
            .position(|window| window == event::PREFIX)
        else {
            continue;
        };
        let frame: Vec<_> = line[offset..]
            .iter()
            .copied()
            .filter(|byte| *byte != b'\r')
            .collect();
        let event = event::decode(&frame).map_err(|error| format!("event frame: {error:?}"))?;
        events.push(event);
    }
    validate(&events)?;
    Ok(events)
}

fn validate(events: &[Event<String>]) -> Result<(), String> {
    let Some(Event::Started { system }) = events.first() else {
        return Err("missing Started event".into());
    };
    if *system == 0 {
        return Err("zero System identity".into());
    }
    if events
        .iter()
        .filter(|event| matches!(event, Event::Started { .. }))
        .count()
        != 1
    {
        return Err("Started must occur exactly once".into());
    }

    let complete = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            Event::Complete { system: owner, .. } => Some((index, *owner)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if complete.len() != 1 {
        return Err("Complete must occur exactly once".into());
    }
    let (complete_at, complete_owner) = complete[0];
    if complete_at + 1 != events.len() {
        return Err("Complete must be last".into());
    }
    if complete_owner != *system {
        return Err("Complete System identity mismatch".into());
    }

    let mut tasks = HashMap::<u64, TaskEvents>::new();
    let mut teams = HashMap::<u64, u64>::new();
    for (index, event) in events.iter().enumerate() {
        match event {
            Event::Started { .. } if index != 0 => {
                return Err("Started must be first".into());
            }
            Event::Bound { task, team, .. } => {
                if *task == 0 || *team == 0 {
                    return Err("Bound task and team must be nonzero".into());
                }
                if tasks.contains_key(task) || teams.insert(*team, *task).is_some() {
                    return Err("Bound task and team identities must be unique".into());
                }
                tasks.insert(
                    *task,
                    TaskEvents {
                        bound: index,
                        ..Default::default()
                    },
                );
            }
            Event::Ready { task } => {
                let state = tasks
                    .get_mut(task)
                    .ok_or_else(|| "Ready without Bound".to_string())?;
                if state.ready.replace(index).is_some() {
                    return Err("Ready must occur at most once per task".into());
                }
            }
            Event::Exited { task, .. } => {
                let state = tasks
                    .get_mut(task)
                    .ok_or_else(|| "Exited without Bound".to_string())?;
                if state.exited.replace(index).is_some() {
                    return Err("Exited must occur at most once per task".into());
                }
            }
            Event::Retired { task } => {
                let state = tasks
                    .get_mut(task)
                    .ok_or_else(|| "Retired without Bound".to_string())?;
                if state.retired.replace(index).is_some() {
                    return Err("Retired must occur at most once per task".into());
                }
            }
            Event::Complete { .. } if index != complete_at => {
                return Err("Complete must occur exactly once".into());
            }
            _ => (),
        }
    }

    for state in tasks.values() {
        let exited = state
            .exited
            .ok_or_else(|| "Bound task has no Exited event".to_string())?;
        let retired = state
            .retired
            .ok_or_else(|| "Bound task has no Retired event".to_string())?;
        if state.bound >= exited {
            return Err("Bound must precede Exited".into());
        }
        if let Some(ready) = state.ready
            && (state.bound >= ready || ready >= exited)
        {
            return Err("Ready must follow Bound and precede Exited".into());
        }
        if exited >= retired {
            return Err("Exited must precede Retired".into());
        }
        if retired >= complete_at {
            return Err("Retired must precede Complete".into());
        }
    }
    Ok(())
}

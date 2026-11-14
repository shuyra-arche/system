//! Fatal retirement of the caller's owned same-Team workers, without the heap.
//! Membership alone does not authorize Team exit. A worker exits itself; its
//! supervisor observes that failure and retires its own workers in turn.
use kernel_api::{
    address::VirtAddr,
    model::{
        identity::TaskId,
        unit::{ExitScope, UnitInfo, UnitQuery, UnitState},
    },
};
use kernel_client::call::{debug, room, unit};

pub fn abort(reason: u64, note: &str) -> ! {
    let _ = debug::put(VirtAddr::new(note.as_ptr().addr()), note.len());
    let _ = debug::put(VirtAddr::new(b"\n".as_ptr().addr()), 1);
    let deadline = room::clock().saturating_add(5_000_000_000);
    let caller = observe(UnitQuery::Caller, deadline).unwrap_or_else(|| retain());
    let mut after = TaskId::new(0);
    // Kernel enumerates the caller's pending administrative records, not all
    // peers. No IDs or permissions are inferred from Team membership.
    while let Some(member) = observe(
        UnitQuery::Members {
            team: caller.team,
            after,
        },
        deadline,
    ) {
        let kernel_api::model::unit::UnitTarget::Task(task) = member.target else {
            retain()
        };
        after = task;
        if member.target == caller.target || member.reclaimed {
            continue;
        }
        loop {
            match unit::stop(member.target) {
                Ok(()) => break,
                Err(error) if error.source.is_busy() => wait(deadline),
                Err(_) => retain(),
            }
        }
        // Keep the root's heap backing and shared handoffs alive until no
        // stopped worker can still access them. Stop alone is not that proof.
        loop {
            let status =
                observe(UnitQuery::Target(member.target), deadline).unwrap_or_else(|| retain());
            if status.state == UnitState::Exited && status.reclaimed {
                break;
            }
            wait(deadline);
        }
    }
    unit::exit(
        0,
        reason,
        VirtAddr::new(note.as_ptr().addr()),
        note.len(),
        ExitScope::Task,
    )
}

fn observe(query: UnitQuery, deadline: u64) -> Option<UnitInfo> {
    let mut bytes = [0; UnitInfo::WIDTH];
    loop {
        match unit::inspect(query, VirtAddr::new(bytes.as_mut_ptr().addr())) {
            Ok(false) => return None,
            Ok(true) => return Some(UnitInfo::from_bytes(&bytes).unwrap_or_else(|| retain())),
            Err(error) if error.source.is_busy() => wait(deadline),
            Err(_) => retain(),
        }
    }
}
fn wait(deadline: u64) {
    if room::clock() >= deadline {
        retain();
    }
    room::yield_();
}
fn retain() -> ! {
    let note = b"system: fatal retirement unsettled; retaining shared backing\n";
    let _ = debug::put(VirtAddr::new(note.as_ptr().addr()), note.len());
    // An unexpected authority/reclaim failure cannot authorize abandoning live
    // workers. Leave the owner and its native grants alive for diagnosis.
    loop {
        room::yield_();
    }
}

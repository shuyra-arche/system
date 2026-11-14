//! System-owned native-memory completion. Accepted work is
//! settled by its original identity; a delivery failure never causes a replay.
use core::sync::atomic::{AtomicU64, Ordering};
use kernel_api::abi::memory::{
    Action, Call, Failure, Outcome, Query, Request, RequestId, Snapshot, Value,
};
use kernel_api::model::nest::{MemoPosition, ReservationPhase};
use kernel_client::{call::room, memory::Response};

// All native-memory owners in the System Team share this monotonic allocator.
// Kernel still scopes journal identities to each actual calling Task.
static NEXT: AtomicU64 = AtomicU64::new(1);
pub fn next_request() -> Option<RequestId> {
    NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .ok()
        .and_then(RequestId::new)
}

pub(crate) struct Memory {
    abort: fn(&'static str) -> !,
}

impl Memory {
    // Instances share NEXT; constructing an owner never resets a Task journal.
    pub(crate) const fn new(abort: fn(&'static str) -> !) -> Self {
        Self { abort }
    }

    fn invoke(
        &self,
        action: Action,
        id: Option<RequestId>,
    ) -> Result<Response, kernel_client::memory::Error> {
        match Call::from_request(
            Request::new(id, action).unwrap_or_else(|| (self.abort)("invalid memory descriptor")),
        ) {
            Call::Flat(call) => kernel_client::memory::flat::invoke(call),
            Call::Nest(call) => kernel_client::memory::nest::invoke(call),
        }
    }

    pub(crate) fn inspect(&self, query: Query) -> Snapshot {
        let deadline = room::clock().saturating_add(5_000_000_000);
        loop {
            if room::clock() >= deadline {
                (self.abort)("memory observation timed out");
            }
            match self.invoke(Action::Inspect(query), None) {
                Ok(Response::Observation(value)) => return value.snapshot(),
                Err(kernel_client::memory::Error::Delivery(Failure::Busy)) => room::yield_(),
                _ => (self.abort)("native memory observation failed"),
            }
        }
    }

    pub(crate) fn mutate(&mut self, action: Action) -> Result<Value, Failure> {
        match self.complete(action) {
            Outcome::Committed(value) => Ok(value),
            Outcome::Rejected(failure) => Err(failure),
            _ => (self.abort)("memory result unsettled"),
        }
    }

    /// Close this entire private Reservation, including partial publication.
    /// A terminal Release(Busy) retains the Reservation. Only this explicit
    /// continuation may use a fresh identity and current version; Pending and
    /// lost deliveries always settle through the original journal entry.
    pub(crate) fn release(&mut self, memo: MemoPosition) -> Result<(), Failure> {
        let deadline = room::clock().saturating_add(5_000_000_000);
        let mut minimum = None;
        loop {
            if room::clock() >= deadline {
                (self.abort)("reservation retirement timed out");
            }
            let Snapshot::Reservation(status) = self.inspect(Query::Reservation {
                token: memo.token(),
                nest: memo.nest(),
                reservation: memo.reservation(),
            }) else {
                (self.abort)("invalid reservation observation")
            };
            if status.nest != memo.nest()
                || status.reservation != memo.reservation()
                || minimum.is_some_and(|version| status.version < version)
            {
                (self.abort)("reservation observation regressed");
            }
            minimum = Some(status.version);
            let action = Action::Release {
                token: memo.token(),
                nest: memo.nest(),
                reservation: memo.reservation(),
                expected: status.version,
            };
            match self.complete(action) {
                Outcome::Committed(Value::ReservationLifecycle {
                    nest,
                    reservation,
                    version,
                    phase: ReservationPhase::Released,
                }) if nest == memo.nest()
                    && reservation == memo.reservation()
                    && version >= status.version =>
                {
                    return Ok(());
                }
                Outcome::Rejected(Failure::Busy | Failure::Stale) => (),
                Outcome::Rejected(failure) => return Err(failure),
                Outcome::Failed {
                    failure: Failure::Busy,
                    retained:
                        Some(Value::ReservationLifecycle {
                            nest,
                            reservation,
                            version,
                            phase: ReservationPhase::Releasing,
                        }),
                } if nest == memo.nest()
                    && reservation == memo.reservation()
                    && version >= status.version =>
                {
                    minimum = Some(version)
                }
                _ => (self.abort)("reservation retirement unsettled"),
            }
            room::yield_();
        }
    }

    fn complete(&mut self, action: Action) -> Outcome {
        let id =
            next_request().unwrap_or_else(|| (self.abort)("memory request sequence exhausted"));
        let original = Request::new(Some(id), action)
            .unwrap_or_else(|| (self.abort)("invalid memory mutation"));
        let mut response = self.invoke(action, Some(id));
        let deadline = room::clock().saturating_add(5_000_000_000);
        loop {
            if let Ok(Response::Reply(reply)) = response {
                if !reply.matches(original) {
                    (self.abort)("uncorrelated memory result");
                }
                match reply.outcome() {
                    Outcome::Pending(_) => {}
                    terminal => return terminal,
                }
            }
            if room::clock() >= deadline {
                (self.abort)("memory result timed out");
            }
            room::yield_();
            response = self.invoke(
                Action::InspectResult {
                    domain: action.opcode().domain(),
                    request: id,
                },
                None,
            );
        }
    }
}

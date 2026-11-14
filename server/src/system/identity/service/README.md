# Identity Task lifecycle

`Service::create` runs in the actual serving Task. It creates 20 distinct Clone
queues for the stable Identity grant set, binds protocol v2 ingress and routes
readiness into one Sign wait group. Each queue admits at most 16 bounded frames.
The group is owned by the migrated supervisor `app::waiting` implementation.
Its controller receives narrowed Push/Peek authority with distribution enabled
for namespace installation. Successful construction exposes the controller-local
identifiers through `publications`; incomplete construction is never a ready
publication receipt. Namespace admission remains Public, Bound or Installer as
defined by the API grant metadata.

`Config` borrows the current Task's CLONE/SIGN creation license, shutdown Sign,
notification Trigger and operator-lifetime grant. The shutdown and notification
owners must match the controller, and the dependency owner must match the
operator. Health checks inspect the actual held grants. They do not assume
ambient Join permission on a parent/sibling numeric TaskId. Operator exit or
withdrawal of its granted lifetime branch wakes the loop and stops progress.

`step` processes at most one frame per endpoint per round, rotates the starting
endpoint and checks health/shutdown between rounds. Invalid envelopes and reply
grants are rejected before dispatch. A retained Open/Return Busy delivery is
retried without waiting for a new queue event. A closed or full caller reply
queue produces an explicit report, and never causes redispatch or service death.
The `run::serve` driver reports reply-delivery failure through native diagnostics.

Successful mutating operations advance the shared epoch before notification and
reply. Queries, Inspect and rejected requests do not advance it. The direct Sign
coalesces notifications: Busy means an event is already pending. The consumer
consumes the exact observed sequence before acquiring the epoch, preserving a
concurrent coalesced update. Notification failure retains pending publication
work before the next request. Epoch exhaustion never wraps and closes progress.

The shutdown Sign requests a graceful stop. `finish` first removes the wait
group, then finishes each ingress and returns queue roots, revoking published
branches. Partial initialization and failed cleanup retain the exact owners for
retry. Stop is not a cancellation or physical-reclamation receipt. The caller
owns the borrowed Config grants. Drop attempts cleanup once; explicit finish is
needed to observe completion. `run::serve` returns a failed live Service to its
caller rather than discarding ownership.

## Verification and limits

The real one-/four-Hart guest runs this production loop with all 20 endpoints.
It checks per-round fairness under a 16-frame malformed flood, authorization
rejection, revision coalescing, read-only queries, a full reply queue after
mutation without replay, repeated SMP RPC, dependency-owner exit, incomplete
construction, graceful shutdown and return of the caller's resource count to
baseline. Native Task exit results and reclaimed status are observed.

An initial SMP run exposed Accord Busy while granting a reply. Client begin now
retries that pre-publication failure on the same reply root and original deadline.

These are native component tests in the System Team. A separate User Team test
confirms that sibling-worker reply delegation is denied by the current Kernel
adjacency gate. [Kernel #140](https://github.com/shuyra-arche/kernel/issues/140)
tracks the authorized cross-Team exchange contract. This module does not weaken
that gate or reply-source checks. Supervisor startup/wait wiring and the other
System services remain unported, so the full executable does not yet build.

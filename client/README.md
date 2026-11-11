# System Client v2

The task-local SDK uses the workspace protocol v2 API, Kernel ABI 9, Twine and
Weave. Each service has a typed `Client` and accepts its Weave operation markers.
Trusted Identity export has a separate client from ordinary Identity calls.

```rust,ignore
use system_client::{control::Client, api::control::{Named, operations::State}, Wait};

let mut client = Client::bind(entry, service_task, creation_license)?;
// On a failed begin, retain Failure and finish failure.cleanup explicitly.
let mut pending = client.begin::<State>(Named { name }, Wait::At(deadline))?;
let result = pending.wait();
// Inspect both outcomes. If cleanup fails, retain pending and retry finish.
let cleanup = pending.finish();
```

`entry` borrows a Clone grant owned by the expected serving Task, with PUSH,
PEEK and Modifiable rights. `creation_license` needs CLONE and SIGN creation
rights. The Kernel checks the live authorities on every call.

Every request gets a fresh single-message reply queue. The SDK grants its Push
authority to the service before encoding the recipient-local reply Pend into
the SYS2 envelope. Twine authenticates the reply's actual sender. Weave checks
the exact frame, and the SDK accepts only the requested response variant.

`begin` retries only a Clone Push Busy that submitted no frame. Sending and
receiving share the original absolute deadline. After a successful submission,
no timeout or receive failure resends the request. `Poll` attempts submission
once. A pending response stays available through `Pending::poll`; a `Poll`
wait returns `NotReady` without abandoning it. A timed wait marks the call
terminal. A queued response can still be accepted when the deadline is reached.

Receive Open Busy retains a dequeued payload in Twine. The SDK yields and
retries that payload rather than waiting for queue readability. This distinction
prevents a wait for a message that has already left the queue.

`Failure` retains resources created before a failed submission. Explicitly
call `failure.cleanup.finish()` and retry failed cleanup. `Pending::finish`
stops further receives and returns watches and the reply root. Failed cleanup
retains the unfinished resource. Destructors are best effort. Closing a route
does not cancel service work already submitted.

## Checks

```sh
cargo +nightly clippy --locked -p system-client --all-targets --target riscv64gc-unknown-none-elf -- -D warnings
cargo +nightly tree --locked -p system-client -e normal,build
python3 tests/guest/client/run.py /path/to/shuyra
```

The guest runs the actual SDK and production `server/src/protocol` ingress on
one and four Harts. It checks source identity, operation mismatch, malformed
frames, Poll, partial initialization cleanup, absolute send/receive deadlines,
late replies, backpressure, route validation before dispatch and shutdown.
It also runs the production Identity state and dispatcher through real calls
for installation, inspection, cancellation, retry and retirement, then observes
the service Task's Kernel exit result and reclamation.
QEMU and OpenSBI can be selected with `QEMU_RISCV64` and `QEMU_OPENSBI`.

The legacy Server's policy and boot paths are still being ported. A passing
isolated ingress guest does not establish an assembled System or a migrated
workspace. The migration branch remains draft while those consumers use their
old interfaces.

# Native internal-service bootstrap

`Workers` owns the Operator/Identity startup sequence. It borrows the initial
Task's creation license, creates dedicated shutdown/revision Signs, prepares
same-Team dormant Tasks and grants only recipient-local Clone/Sign rights.
`Slot` retains a stable handoff allocation until the Kernel proves that Task
reclaimed. Release/acquire publication protects both configuration and receipts;
no closure trampoline, scanned Mark or old capability conversion participates.

Operator publishes three caller-local service entries and a native health Sign.
Identity receives a narrowed health branch and publishes twenty typed entries.
The supervisor submits Root once, then grants Resolve/Matches/Same to Operator
and submits Wired once. `advance` polls those requests and retains reply cleanup;
a failed delivery does not repeat an already submitted bootstrap mutation.
Publications become available after acknowledgment and local reply cleanup.
One absolute five-second deadline bounds the startup sequence.

`finish` retains partial startup state. It notifies Identity once and observes
its exit/reclamation before notifying Operator, preserving dependency order.
Repeated rounds never flood live or retiring subscriptions with new shutdown
notifications. At the grace deadline, unfinished Tasks receive Stop; Stop is
not a reclaim receipt. Only after reclamation may the supervisor discard worker
records, handoffs and its published/owned Pends. Failed cleanup remains owned
for another round. Dropping an unfinished Slot conservatively retains its
handoff if reclamation cannot be proven, preventing a cross-Hart use-after-free;
this fallback is not successful cleanup.

Unexpected internal Task death fails startup/health checks. Capability cleanup
is possible, but native forced stop cannot run arbitrary Rust destructors in the
dead Task. The enclosing System must treat this as fatal to its Team rather than
restart an internal worker and claim its language heap was reclaimed. Ordinary
Units have separate Teams and independent retirement handling.

The actual production objects run in `tests/guest/client` on one/four Harts:
empty/partial cancellation, insufficient rights, both native service roles,
Root/Wired, live Identity resolution and revision, Operator namespace use,
ordered clean worker exits, unexpected worker death and caller capability
baseline recovery. This verifies the bootstrap component; the assembled Server
application, allocator/entry and remaining legacy service wiring are still being
ported and are not represented by this test guest.

# Native internal-service bootstrap

`Workers` owns the Operator/Identity startup sequence. It borrows the initial
Task's creation license and native memory context, creates dedicated shutdown/revision Signs, prepares
same-Team dormant Tasks and explicitly grants recipient-local Clone/Sign rights
and narrowed INSPECT/RESERVE access to their actual Internal Nest. Each worker
uses the shared native request allocator; the supervisor retains its original
context for setup replies.
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

The historical Client guest exercised these objects on older providers, including
partial cancellation, both roles, Root/Wired and ordered retirement. Current
ABI 11 migration adds explicit context and Nest handoff; that comprehensive guest
and the full bootstrap/entry path have not passed on the new providers. Current
runtime evidence in `tests/guest/runtime` covers the heap and same-Team
Client/Ingress/Worker boundary only. It does not launch Operator/Identity or the
complete System supervisor.

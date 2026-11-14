# Native Supply sources

`native::Source` owns registration roots, private lease roots, delivery receipts,
cancellation tombstones and native Return progress. Inspect/replay never creates
a second grant; accepted work remains owned across Busy and withdrawal. A
successful Return of a private root proves its branch has been withdrawn.

`Registry` borrows real System-owned roots and records an explicit maximum. The
application registers boot ledger grants and restricted service queues; it does
not infer resource authority from a name, mark or device address. Installation
leases use System as their authenticated requester and the actual child Task as
target. Each instance retains its Sources until explicit cleanup completes.

The candidate RPC adapter checks actual ingress caller, declared source/resource,
slot and rights, restricts Acquire to that caller, and correlates `(caller,
LeaseId)` with the owning Source. A source maximum may not exceed the Registry
maximum. Dead callers retain their native Source through withdrawal. This
bounded adapter is part of the pending S1/S2 contract review; it is not a second
device ledger or a frozen Hub handoff.

The [runtime guest](../../../../tests/guest/runtime/README.md) validates the
native backend's rights, replay, cancellation, cleanup and relative windows.
The [application scenarios](../../../../tests/guest/system/README.md) also use
actual declared service leases before User activation and reject missing
sources. RPC pressure, provider failure and full public handoff remain distinct
gates; the old hardware hooks are retained only as historical test sources.

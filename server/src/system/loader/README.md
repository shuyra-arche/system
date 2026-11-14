# Native image construction

`Loader` owns a bounded immutable-source cache and borrows the creation license
and its actual current Internal Nest. Its unsafe constructor requires that
current-Nest proof: source staging dereferences private writable projections.
`Job` owns one ELF construction, its Team/Task receipts, source references and
target reservations. Every `advance`/`finish` performs finite progress and keeps
acquired objects after Busy, failure or cancellation.

Source preparation uses native CreateFlat/Occupy, a private writable reservation,
a bounded copy into zeroed backing, then explicit Release/Return. A derived Share
is read-only for immutable segments; writable DATA/BSS gets a private source.
The backing Flat stays owned through every target projection and live source
reference. Final cleanup returns the Share, closes the Flat and returns its
closed metadata Pend. No retired Cell or Share Open/Shut call participates.

The common runtime `Mapping` owns Reserve/Insert/Publish/Release/Return and
observes actual versions before mutation. Flat slots and Share windows retain
their original source coordinates. All owners use the Team's shared monotonic
native request allocator and query accepted operations by their original ID.
An address in a foreign Nest is descriptive; only the trusted local staging
owner may dereference its own mapping.

ABI 11 permits the creator's genuine Nest authority to prepare the complete
image before Task creation. The child starts at the ELF entry with its original
arguments and Kernel stack, with no injected System initializer or consumed
native RequestIds. Application readiness remains a separate supervisor/service
fact. Trusted construction policy explicitly selects the creation rights handed
to the child; the child also receives only Whole-Nest INSPECT/RESERVE. The ordinary
Team path rejects Supervisor images.

RX cache keys compare exact bytes, prefix, span and flags. Every Job retains a
source reference, so eviction cannot withdraw another instance's code. RW
sources are private. Stop requests termination; Task `reclaimed`, explicit
mapping retirement and successful Team Delete are separate completion facts.
The Job preserves its Task result before Team Delete clears descendant receipts.
Only after these facts may the last source owner close backing. Team
`UnitInfo.reclaimed` is not used as a Task reclamation substitute.

The [Loader guest](../../../../tests/guest/loader/README.md) runs actual User
Teams, partial/dormant cancellation, arguments, private DATA/BSS, RX reuse and
live eviction, first native RequestId, result 17, Task/Team retirement and Pend
baseline recovery on the production heap. It is not assembled System or Toolchain
std acceptance. Loader RPC/image admission and Control/Assembly integration
remain to be migrated.

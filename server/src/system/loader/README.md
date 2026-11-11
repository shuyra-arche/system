# Native image construction

`Loader` owns a bounded immutable-source cache. `Job` owns one construction,
its Team/Task records, sources, CPU Memos and initializer resources. Each
`advance` performs bounded progress and retains acquired objects on failure.
`Mapping` observes generations immediately before Insert/Publish. No legacy
Memory/Transfer call or untyped Task/Team operation participates.

Writable mappings follow Kernel 9's receiving-Task rule. The parent installs
only read-only executable projections. It creates the initial Task dormant,
then grants its private RW sources and authenticated read-only metadata. The
initial Task runs `bootstrap::enter`, associates/publishes RW DATA/BSS in its own
space, releases every metadata borrow and triggers a fresh initializer Sign
before tail-jumping with the original arguments and Kernel stack bounds. The
one-time metadata search verifies actual creator, owner, lender, Mark, native
kind, status and limits. It is not service discovery or a persistent session.
Temporary Share Open mappings are closed before reserving a program DATA range:
the temporary mapping may otherwise occupy precisely that target address.

The native entry linker must expose the complete immutable `__code_start` to
`__code_end` span. The initializer and its Rust/SDK callees share that RX span
at the same VA in the target, never the System's writable data. The entry code
is placed at a high VA; ELF ranges colliding with this reserved bootstrap span
are rejected. This is currently a whole-RX projection, not a small standalone
trampoline. It conveys no administrative capabilities. An initializer failure
uses reason `0x4c4f4144`; its diagnostic result identifies the failing source
location or Kernel Cell error and is not a stable public protocol code.

Application readiness is distinct from initializer quiescence. `initialized`
retains Sign consumption and cleanup progress and lets the creator release its
metadata only after all target-side borrows end. The application may already
have exited; no debug line substitutes for a Kernel receipt.

RX cache entries compare the exact bytes, prefix, span and flags. Every live
Job retains its source, so eviction cannot withdraw another Unit's code. RW
sources are private. Retirement releases the native root Task receipt only
through the completed Team deletion path. ABI 9 does not set Team
`UnitInfo.reclaimed`: that field is meaningful for Tasks. Successful Team Delete
coordinates Nest retirement, returning Busy until safe, and clears the caller's
retained descendant receipts. Job preserves its Task result before deletion,
then finishes creator-owned Memos, sources and initializer resources. Stop alone
is not reclaim evidence.

The QEMU guest runs actual User Teams with a separate compiled RV64 image. It
checks copied arguments, initialized/private DATA, zero/private BSS, execution
from reused RX, cache eviction while another Unit remains dormant, partial
construction cancellation, native exit result 17, Task reclamation and caller
capability baseline recovery. One/four-Hart runs include every previous service
and bootstrap test. This is production Loader component evidence, not an
assembled System or Toolchain `std` acceptance result. Loader RPC admission,
image import and remaining Control/Assembly integration still need migration.

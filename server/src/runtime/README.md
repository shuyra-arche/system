# Native initial Task and heap

The System binary uses Kernel ABI 11's word arguments directly. `main.rs` validates the
four-word shape, borrows the current boot ledger, initializes the heap from its
explicit Create Pend and trusted Space grant, opens the DTB, and invokes System on the initial guarded
stack. Panic formatting uses stack storage and the native debug call. `link.ld`
keeps immutable RX and writable data separate and exports the RX bounds required
by the Loader initializer. `build.rs` no longer consumes Execution metadata.

The Talc heap grows through explicit native Flat holdings and Nest Reservations.
Backing controls always belong to the initial Task. Workers share the allocator
without receiving its creation license; exiting a worker cannot withdraw live
heap pages. Completely free backing heaps are retracted, disassociated, released and deleted; Talc may retain
its first heap for metadata. This is not the fixed bump heap in older fixtures.

The initial supervisor must call `heap::poll` between finite rounds, including
startup and shutdown, and must not wait indefinitely while workers can allocate.
A worker holds the Talc lock while publishing a backing request. ROOT services
that mailbox without allocation, both at polls and while contending for the
heap lock. Acquire/release publication separates the request and response. ROOT
rechecks lock ownership after inspecting a departed lock holder, so an ordinary
unlock/exit race does not become a false fatal error. Busy inspections retry.
An internal Task dying while still owning the allocator lock is fatal to the
whole System Team; this does not claim recovery of a partially mutated heap.

All native mutations share `memory::next_request`. Delivery failure queries
the original result instead of submitting new work. Only a rejected Busy/Stale
may obtain a fresh request after refreshing the expected version. Accepted
failure or an unresolved deadline terminates the enclosing Team, leaving the
Kernel responsible for retained physical obligations.

Current ABI 11 runtime evidence lives in `tests/guest/runtime`: native heap,
Supply and same-Team Client/Ingress calls on the single-Hart qualified platform.
The old entry fixture and complete application still require migration. The
selected Kernel rejects native memory qualification with multiple Harts; no
fixture marker can substitute for its missing remote drain mechanism.

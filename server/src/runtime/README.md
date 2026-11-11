# Native initial Task and heap

The System binary uses Kernel9's word arguments directly. `main.rs` validates the
four-word shape, borrows the current boot ledger, initializes the heap from its
explicit Create Pend, opens the DTB, and invokes System on the initial guarded
stack. Panic formatting uses stack storage and the native debug call. `link.ld`
keeps immutable RX and writable data separate and exports the RX bounds required
by the Loader initializer. `build.rs` no longer consumes Execution metadata.

The Talc heap grows through private Cell Slots and generation-qualified Memos.
Backing controls always belong to the initial Task. Workers share the allocator
without receiving its creation license; exiting a worker cannot withdraw live
heap pages. Completely free backing heaps are retracted/deleted; Talc may retain
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

Verification lives in `tests/guest/runtime` and `tests/guest/entry`. The entry
fixture compiles the exact production `main.rs`, linker, boot decoder and heap;
it starts and retires the actual production Operator/Identity workers on this
heap. Its application body is a contract fixture, not the complete System
supervisor. Control/Assembly and the remaining old integration still prevent the
normal Server build; these tests do not mark M3/M4 or System #10 complete.

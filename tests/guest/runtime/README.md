# Native heap and Supply leases

Uses production `server/src/runtime/heap` as its global allocator and production
Supply lease backend. No fixed fixture arena or old Execution allocator is used.
Four real Tasks transfer live allocations to the initial Task, exit with distinct
native results, and are observed reclaimed/deleted. The initial Task still reads
all transferred bytes; complete frees restore the backing-mapping baseline. ROOT
also allocates during worker contention; a later three-MiB allocation exercises
growth beyond the older fixture budget.

Supply checks use real Kernel Pends: rights and recipient scope, identical versus
conflicting replay, cancellation before/after derivation, out-of-order IDs,
source retention and complete withdrawal. A mapped source with distinct page
contents verifies a two-stage relative window is applied exactly once.

```sh
export QEMU_RISCV64=/path/to/qemu-system-riscv64
export QEMU_OPENSBI=/path/to/fw_jump.bin
python3 tests/guest/runtime/run.py /path/to/shuyra
cargo clippy --locked --manifest-path tests/guest/runtime/Cargo.toml --target riscv64gc-unknown-none-elf -- -D warnings
```

Runner requires all three markers on one/four Harts, with a 120-second wall-clock
bound per guest. Source/SDK pin is Kernel `23dec6b30e1ec08f42d3701e2613acd8c562c459`.
This tests actual native resource ownership; the full Supply RPC/application,
Control supervisor, external providers and device retirement are separate gates.

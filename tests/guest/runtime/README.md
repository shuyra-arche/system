# Native heap, Supply and RPC

Uses the production System heap, native memory result owner, Supply lease
backend, Client and authenticated ingress. Four real Tasks transfer live
allocations to the initial Task, exit with distinct results and are observed
reclaimed/deleted. ROOT reads those allocations after their original Task exits;
complete frees restore both backing mappings and the Task-local Pend baseline. ROOT also allocates under
worker contention; a later three-MiB allocation exercises heap growth.

Supply checks real rights, caller/recipient scope, identical versus conflicting
replay, cancellation before/after derivation, delayed IDs, original-source
retention and complete withdrawal. Eight distinct page contents verify that two
relative narrowing steps deliver original page 2, once, through an actual native
read-only publication. Publication retirement precedes lease withdrawal.

RPC makes two actual protocol-v4 calls using production Client/Ingress and a
production native Worker. The caller and worker explicitly share their Team's
Internal Nest through recipient-local grants. Heap and reply mapping use the
same monotonic request allocator. The guest checks actual request/reply senders,
typed responses, retained cleanup, exit result 31 and Kernel reclamation.
This is a same-Team component scenario, not independent-Team or full System
acceptance.

## Reproduce

Select the repository's nightly and RV64 target. Build the explicit CPU-only
QEMU platform fixture, which pins Vanllia `b5468739f2d7e36d64198de743a13d6ccabc6666`
and Kernel `b2eb37e2af134e4bb4248df454c437b06c1f93c0`:

```sh
RUSTFLAGS='-Crelocation-model=static -Cforce-frame-pointers=yes -Ccode-model=medium' cargo build --locked --manifest-path tests/guest/runtime/platform/Cargo.toml --release --target riscv64gc-unknown-none-elf
export QEMU_RISCV64=/path/to/qemu-system-riscv64
export QEMU_OPENSBI=/path/to/fw_dynamic.bin
python3 tests/guest/runtime/run.py tests/guest/runtime/platform/target/riscv64gc-unknown-none-elf/release/system-runtime-platform --harts 1
cargo clippy --locked --manifest-path tests/guest/runtime/Cargo.toml --target riscv64gc-unknown-none-elf -- -D warnings
```

The runner requires all four markers and successful QEMU termination. Its default
still requests both one and four Harts; `--harts` selects an explicit subset.
Each guest has a 120-second wall-clock bound and finite progress deadlines.
The fixture attests coherent CPU RAM only under the paired QEMU virt invocation,
with 128 MiB, no hotplug/passthrough and no External routes. It qualifies no
executable publication. This is not production platform admission.

## Remaining gates

The selected Kernel rejects all native memory qualification with four Harts in
`server/src/metal/riscv/memory.rs::install`: remote SFENCE acknowledgements alone
cannot prove remote data accesses have drained. Consequently `--harts 4` fails
before the guest starts, and the default combined run is not passing acceptance.
The unqualified normal Vanllia platform rejects native Publish as Unsupported.
Preserve both fail-closed boundaries; do not synthesize completion evidence.

Full Supply RPC/provider collections, Control/Instance/UnitRoot integration,
Loader and Toolchain startup, complete System and device retirement remain
separate gates in System #13 / Vanllia #39. Older entry/client guest evidence
must be rerun after those fixtures and their production owners finish migration.

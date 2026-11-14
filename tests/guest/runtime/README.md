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
read-only publication. Publication retirement precedes lease withdrawal. The production read-only
projection owner is also cancelled at each partial-open phase; each close
restores the Pend baseline and leaves its borrowed source active.

RPC makes two actual protocol-v4 calls using production Client/Ingress and a
production native Worker. The caller and worker explicitly share their Team's
Internal Nest through recipient-local grants. Heap and reply mapping use the
same monotonic request allocator. The guest checks actual request/reply senders,
typed responses, retained cleanup, exit result 31 and Kernel reclamation.
This is a same-Team component scenario, not independent-Team or full System
acceptance.

## Reproduce

Select the repository's nightly and RV64 target. Build the explicit CPU-only
QEMU platform fixture, which pins Vanllia `58498c13f41ad64a1fc09af338802b279347c9c8`
and Kernel `684f479c45dc2d94d405b32b4ffc6334a4233fc9`:

```sh
RUSTFLAGS='-Crelocation-model=static -Cforce-frame-pointers=yes -Ccode-model=medium' cargo build --locked --manifest-path tests/guest/runtime/platform/Cargo.toml --release --target riscv64gc-unknown-none-elf
export QEMU_RISCV64=/path/to/qemu-system-riscv64
export QEMU_OPENSBI=/path/to/fw_dynamic.bin
python3 tests/guest/runtime/run.py tests/guest/runtime/platform/target/riscv64gc-unknown-none-elf/release/system-runtime-platform --harts 1 4
cargo clippy --locked --manifest-path tests/guest/runtime/Cargo.toml --target riscv64gc-unknown-none-elf -- -D warnings
```

The runner requires all four markers and successful QEMU termination. Its default
still requests both one and four Harts; `--harts` selects an explicit subset.
Each guest has a 120-second wall-clock bound and finite progress deadlines.
The fixture attests coherent CPU RAM only under the paired QEMU virt invocation,
with 128 MiB, no hotplug/passthrough and no External routes. It qualifies no
executable publication. This is not production platform admission.

## Remaining gates

The selected Kernel uses explicit remote data fences, translation invalidation
and generation-bound acknowledgements on all ever-online Harts. Only complete
CPU rounds permit reclamation; Busy or bounded failure retains ownership.
Twine explicitly continues a retained Release(Busy) with a fresh request while
preserving the original failed receipt. CI exercises this data-only fixture on
one and four Harts. This data-only platform still rejects executable memory;
the separate `instructions` feature qualifies the Loader 1/2/4-Hart scenario.
The unqualified normal Vanllia platform rejects native Publish as Unsupported.
Preserve both fail-closed boundaries; do not synthesize completion evidence.

Full Supply RPC/provider collections, Control/Instance/UnitRoot integration,
Toolchain startup, complete System and device retirement remain
separate gates in System #13 / Vanllia #39. The [Loader guest](../loader/README.md) separately checks native ELF construction
and independent User Team retirement; its explicit platform feature additionally
qualifies single-Hart local instruction visibility. The entry guest checks the production entry, native DTB/initrd and
internal-service lifecycle. The broad client guest still needs migration.
Neither fixture substitutes for the complete System application.

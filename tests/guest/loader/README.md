# Native Loader lifecycle

This guest includes the production Loader, native projection/result owners and
production heap. It creates actual independent ordinary User Teams from a
separately compiled RV64 ELF. The root fixture uses the public Kernel startup
ledger; it does not run the complete System application.

The image checks copied and empty argument blocks, writable initialized DATA and zeroed BSS,
then uses the production native-memory result owner to Create/Delete a Flat.
Its first mutation uses RequestId 1: parent construction cannot consume or reset
the new Task's native journal. Two instances reuse immutable RX while retaining
private DATA/BSS; cache eviction must leave the dormant instance executable.
Cancellation covers partial source/target construction and dormant instances.
A real image Trigger proves it reached its native allocation before the parent
Stops a running Team and observes explicit native reclamation. Task results and reclaimed observations precede
Team deletion; only then can the final source owner release backing. Temporary
reservations and closed metadata Pends must return to the caller baseline.
Controller allocations are dropped before comparing the production heap's Pend
inventory.

Build the explicit single-Hart instruction-qualified test platform:

```sh
export RUSTFLAGS='-Crelocation-model=static -Cforce-frame-pointers=yes -Ccode-model=medium -Dwarnings'
cargo build --locked --manifest-path tests/guest/runtime/platform/Cargo.toml --features instructions --target-dir tests/guest/runtime/platform/target/instructions --release --target riscv64gc-unknown-none-elf
export QEMU_RISCV64=/path/to/qemu-system-riscv64
export QEMU_OPENSBI=/path/to/fw_dynamic.bin
python3 tests/guest/loader/run.py tests/guest/runtime/platform/target/instructions/riscv64gc-unknown-none-elf/release/system-runtime-platform --harts 1
```

The runner builds both fixture and image with `--locked`, requires the Loader
marker and Kernel's all-tasks-exited shutdown, and bounds execution to 120
seconds. Its default includes four Harts; that topology remains rejected, as
tracked in Kernel #179. The instruction qualification covers the paired
one-Hart coherent QEMU invocation with no External requester, using local
FENCE.I. No SMP or physical-hardware claim follows.

This is independent-User-Team **Loader component** evidence. Public Control,
Operation/Instance ownership, authenticated Create policy, initial service
capability bundles, a Toolchain std image and the complete System/Vanllia product
remain separate acceptance gates. Supervisor construction remains rejected by
this ordinary-Team Loader path.

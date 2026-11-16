# Native Loader lifecycle

This guest includes the production Loader, native projection/result owners and
production heap. It creates actual independent ordinary User Teams from a
separately compiled RV64 ELF. The root fixture uses the public Kernel startup
ledger; it does not run the complete System application.

The User image checks copied and empty argument blocks, writable initialized
DATA and zeroed BSS, then uses the production native-memory result owner to
Create/Delete a Flat. Its first mutation uses RequestId 1: parent construction
cannot consume or reset the new Task's native journal. Two instances reuse
immutable RX while retaining private DATA/BSS; cache eviction must leave the
dormant instance executable. Cancellation covers partial source/target
construction and dormant instances. A real image Trigger proves it reached its
native allocation before the parent Stops a running Team and observes explicit
native reclamation. Task results and reclaimed observations precede Team
deletion; only then can the final source owner release backing. Temporary
reservations and closed metadata Pends must return to the caller baseline.
Controller allocations are dropped before comparing the production heap's Pend
inventory.

## Run

The candidate Rust host runner builds the Loader fixture and its ordinary User
image under `server/tests/target/loader/`. Pass a matching Kernel ELF and
profile metadata. The ordinary User ELF remains release in both Loader
profiles; CI's native matrix uses the `instructions` platform.

```sh
export QEMU_RISCV64=/path/to/qemu-system-riscv64
export QEMU_OPENSBI=/path/to/fw_dynamic.bin
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- loader --kernel PATH --output NEW --profile release --kernel-profile release --harts 1 2 4
```

For the debug Kernel and Loader pair, pass the debug Kernel ELF and use
`--profile debug --kernel-profile debug`. Each VM has a 120-second bound. The
runner must require the Loader marker, no `FAIL` marker, successful QEMU exit,
and Kernel's all-tasks-exited shutdown. This preserves the existing six-case
Kernel/Loader profile × Hart matrix. Evidence is written to the caller's
`--output` directory; retain failed receipts.

The instruction qualification covers paired coherent QEMU `virt` CPUs with no
External requester, hotplug or unmanaged executable aliases. Kernel completes
an independent all-Hart FENCE.I round before installing executable PTEs. It
retains a page-wide W/X guard throughout that wait and revalidates
authorization before publication. Hardware and device instruction fetch
remain unverified.

This is independent-User-Team **Loader component** evidence. Public Control,
Operation/Instance ownership, authenticated Create policy, initial service
capability bundles, a Toolchain `std` image and the complete System/Vanllia
product remain separate acceptance gates. Supervisor construction remains
rejected by this ordinary-Team Loader path. The previously optional
Toolchain-receipt execution is not part of the candidate CLI documented here.

The cache/independent-data pair also compares an owned image snapshot with a
borrow of an immutable static image. Catalog borrowing avoids copying and
retiring a complete ELF for every attempt; anonymous inputs still require an
owned snapshot. Both use the same parser, native construction and retirement.

The fixture is under active runner migration. These commands do not claim that
the Rust runner is implemented, that old/new parity has passed, or that the
System product has been accepted. Keep the existing `native.yml` checks until
the candidate has demonstrated equivalent evidence and failure behavior.

# Production System application

This runner builds `system-server --bin system` from the root workspace. It
packages that exact production binary and separately compiled ordinary User
ELFs in pure schema-2 records; it does not substitute a fixture implementation
for `system_server::run`.

The release scenarios are:

- An empty startup catalog boots and retires the real Operator/Identity workers.
- Two transient ordinary User Teams receive declared narrow Clone Supply grants
  and per-instance namespace grants. Each image checks its private DATA/BSS,
  copied/empty arguments and fresh native request journal before returning 17.
  Actual Task exit and reclaimed observations, namespace/lease/identity cleanup,
  target mapping retirement and Team deletion precede the retired event.
- An unknown declared Supply resource rejects installation. The dormant Task
  never returns 17; its real execution and backing are retired, and System emits
  a nonzero Complete event instead of reporting startup success.

The host pack/check helper decodes the actual typed lifecycle event frames and
checks ordering and independent Task/Team identities. The runner also requires
Kernel shutdown, the appropriate System outcome, and no guest page fault/panic.
These observations are backed by the production state owners' native checks;
log text alone is not a physical-retirement proof.

Use the instruction-qualified platform from the [Loader scenario](../loader/README.md):

```sh
cargo build --locked --manifest-path tests/guest/runtime/platform/Cargo.toml --features instructions --target-dir tests/guest/runtime/platform/target/instructions --release --target riscv64gc-unknown-none-elf
export QEMU_RISCV64=/path/to/qemu-system-riscv64
export QEMU_OPENSBI=/path/to/fw_dynamic.bin
python3 tests/guest/system/run.py tests/guest/runtime/platform/target/instructions/riscv64gc-unknown-none-elf/release/system-runtime-platform
```

The runner always selects one Hart. The provider qualification is limited to
this coherent QEMU invocation, local FENCE.I and no External requester. It does
not establish SMP or hardware behavior.

`--profile debug` builds and runs the unoptimized production binary. Compilation
passes, but its repeated-install/worker-retirement execution currently exposes
an Operator page fault inside the native transport return path. This is a real
failing scenario, retained for diagnosis; it is not a required passing CI check.
Identity subsequently loses its Operator dependency. The failure's cause is
not yet established as System, compiler or provider behavior. Release passing
does not prove debug runtime correctness.

The User image is a native `no_std` test ELF, not a Toolchain std Unit. Full
System/Toolchain acceptance, public Create/Claim/InstanceId/UnitRoot handoff,
cross-Team application RPC, recovery and fault campaigns remain S1–S3 work.

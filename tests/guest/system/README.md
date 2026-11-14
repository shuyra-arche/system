# Production System application

This runner builds `system-server --bin system` from the root workspace. It
packages that exact production binary and separately compiled ordinary User
ELFs in pure schema-2 records; it does not substitute a fixture implementation
for `system_server::run`.

The same scenarios run with either System build profile:

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
Kernel shutdown, the appropriate System outcome, Kernel's admitted root Task
exit with that same reason/note, and no guest page fault/panic. A Complete event
printed before a rejected terminal call is insufficient.
These observations are backed by the production state owners' native checks;
log text alone is not a physical-retirement proof.

Use the instruction-qualified platform from the [Loader scenario](../loader/README.md):

```sh
cargo build --locked --manifest-path tests/guest/runtime/platform/Cargo.toml --features instructions --target-dir tests/guest/runtime/platform/target/instructions --release --target riscv64gc-unknown-none-elf
export QEMU_RISCV64=/path/to/qemu-system-riscv64
export QEMU_OPENSBI=/path/to/fw_dynamic.bin
python3 tests/guest/system/run.py tests/guest/runtime/platform/target/instructions/riscv64gc-unknown-none-elf/release/system-runtime-platform
```

The default selects one Hart. `--profile debug` selects the unoptimized System
binary; the separately compiled User ELF remains the fixed release fixture.
`--harts 1 2 4` is available for an explicitly SMP-qualified Kernel platform;
it does not make an older single-Hart-only provider support SMP. Provider
qualification covers paired coherent QEMU virt CPUs with no External requester,
not hardware or device retirement.

The production entry uses a separate 1 MiB, 16-byte-aligned RW/non-executable
PT_LOAD stack before entering Rust. It retains the original a0/a1 word pointer
on the Kernel-owned bootstrap stack for this Task's lifetime. An absent page
below the new stack is outside all PT_LOADs and below the Kernel's post-image
Normal allocation window. Native reservation admission cannot populate it.
The original 64 KiB stack overflowed the debug supervisor call chain and could
corrupt a neighboring internal worker stack before a useful fault. This fix
changes neither ABI arguments nor the memory request sequence. The entry
fixture checks the actual current stack; this runner checks the ELF guard/load
layout and executes the full application without relaxing its assertions.

Heap backing now retires its entire private Reservation through Release,
retaining its Pend and Flat until a committed Released receipt. A terminal
Failed(Busy, Releasing) keeps that owner and failed journal receipt; the next
explicit Release uses a fresh request and observed version. Pending or lost
delivery instead queries the original ID. It never returns storage early.
This avoids the old per-page Retract failure while the root held Talc's lock.

Normal completion exits only the initial Task after service retirement. Fatal
retirement allocates no memory: it stops the caller's administratively owned
same-Team workers, observes actual reclamation, then exits the Task. It does not
assume that Capsule Team membership permits ExitScope::Team. An unexpected
stop/reclaim failure leaves the owner and shared backing alive for diagnosis.

Bootstrap keeps its five-second inactivity bound. Completed resource/grant
acquisitions, acknowledged phases and finite worker initialization steps renew
that bound. The worker publishes its completion time; repeated reads, Busy,
yields and stale wakeups do not renew it. The entry scenario also freezes a
startup worker to prove that a stalled bootstrap still times out.

Run both System profiles against both recorded Kernel profiles with
`--harts 1 2 4`. The full matrix and CI preserve separate receipts; passing
these native User ELF scenarios does not complete S1/S2 or Toolchain acceptance.

`target/evidence/<profile>/` retains each serial log and JSON result, exact
System/User/Kernel/firmware hashes, profile selection, locks, ELFs and archives.
A timeout or failed assertion leaves a failed receipt. CI uploads this evidence.

The User image is a native `no_std` test ELF, not a Toolchain std Unit. Full
System/Toolchain acceptance, public Create/Claim/InstanceId/UnitRoot handoff,
cross-Team application RPC, recovery and fault campaigns remain S1–S3 work.

The separate `--handoff` mode uses the [independent-Team consumer](../handoff/README.md)
against this same production Server. It exercises the Instance v1 subset,
Operator UnitRoots, revision CAS, direct Driver RPC, native restricted lease
withdrawal, replay/lost reply/cancellation and early owner exit. Its evidence
lives under `target/handoff/`; it does not mark the complete S1/S2 or Toolchain
acceptance checklist passed.

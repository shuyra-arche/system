# System

System boots and supervises Vanllia Units. It owns execution, identity,
publication and retirement; hardware enumeration and driver policy belong to Hub.
The host Vanllia CLI selects Unit records and images for the boot archive.

This is the ongoing migration in [System #13](https://github.com/shuyra-arche/system/issues/13).
[PR #17](https://github.com/shuyra-arche/system/pull/17) consolidates the native
application, provider selection and retirement fixes from #12, #15 and #16.
[Vanllia #39](https://github.com/shuyra-arche/vanllia/issues/39) owns Track S
progress and assembled-system acceptance. **The complete Server now builds in
RV64 debug/release. S1–S3 and full product acceptance remain incomplete.**

## Code map

| Entry | Responsibility |
| --- | --- |
| `api/` | Protocol v4 codecs and typed service contracts |
| `client/` | Task-local clients and retained reply cleanup |
| `server/src/protocol/` | Authenticated ingress before service effects |
| `server/src/runtime/` | Initial-Task heap and native memory result ownership |
| `server/src/system/` | Services, declarations and supervisor migration; [`control/`](server/src/system/control/README.md) documents Control and construction receipt ownership |
| `support/` | Borrowed boot archive, typed ledger and bounded platform facts |
| `tests/` | Host contracts and explicit target component fixtures |

Production packages keep `no_std`. Test code belongs under `tests/`.
The [external Supply registration contract](docs/supply.md) adds caller-scoped
source registration/withdrawal and caller-owned installation into an actual
Prepared Instance before Start.
The [Instance handoff v1](docs/instance.md) adds caller-owned instance operations,
queryable receipts and Operator UnitRoots for the Hub integration. Its native
[independent-Team fixture](tests/guest/handoff/README.md) is a provider increment;
the full S1/S2 checklist and downstream acceptance remain open.
Read [AGENTS.md](AGENTS.md), [CONTRIBUTING.md](CONTRIBUTING.md) and
[PROVENANCE.md](PROVENANCE.md) before changing this repository.

## Selected native providers

| Provider | Exact revision |
| --- | --- |
| Kernel API/Client | `789ffd19b5359f3de172fb9aad8a61f6a7113a64` (ABI/Ledger 13/13, Capsule 2) |
| Twine | `932454c07bd2673ac7d6d9612d36182aae71e8b2` |
| Weave | `7a7d67fb3f190aeb43049d051b0972bcd53213bc` |
| Unit declarations | `aaa2b58c1ef0b40428dcc71d1321736156551109` (schema 2, runtime disabled) |

This candidate coordinates Kernel #160/#206/#188 and the API macro relocation.
API/Client/Server and Twine share the published full Kernel SHA above; the test
platform selects Vanllia `0eb086a2416eee8921416b12bf94819b30b6bb1f`.
Land Kernel, then Twine/System, then Vanllia composition. No release is implied.
ABI 11/12 images and old Task descriptors require a coordinated rebuild.
Every external Capsule and ordinary Task executes in User mode through ECALL;
Supervisor execution is restricted to Kernel internal work.

System Loader now prepares target-Nest stack backing, argument words and an
unmapped lower guard before submitting TaskStart. Runtime workers likewise use
initial-Task-owned guarded heap backing. Worker/Loader records retain their real
memory owners through creation failure, cancellation, Task reclamation and
separate mapping retirement; Task exit alone never releases stack storage.
The stack guard separates usable stack storage from its allocator metadata.
System #18 continues to own future PIE/ASLR policy.

API/Client 0.4 and protocol v4 reject v2/v3 envelopes and retired Share Open/Shut
rights. A client or ingress receives an explicit shared memory context backed by
its actual Internal Nest. Its allocator must also serve the Task's other native
memory users. See [Client lifecycle](client/README.md).

The complete production graph selects these native providers and pure Unit
schema 2. Old Env/Wire/Mold/IPC/Resource/Execution dependencies, Cargo
replacements and the System API self Git pin have been removed. The original
Identity/Operator/Loader/Supply mechanisms are retained and composed by the
[native application](server/src/system/app/README.md). Inactive SDK harnesses
remain explicitly [historical](tests/historical/README.md). The finite
[test and support inventory](docs/test-inventory.md) records owner migrations
and remaining assertion/consumer work.

## Verification

Use `rust-toolchain.toml` (`nightly-2026-11-13`, RV64 target, Rust source,
Rustfmt and Clippy). From this repository root:

```sh
cargo test --locked -p system-api -p system-client -p system-support --tests --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path server/tests/Cargo.toml --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path tests/identity-state/Cargo.toml --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path tests/operator-policy/Cargo.toml --target x86_64-unknown-linux-gnu
cargo clippy --locked -p system-client -p system-support --lib --target riscv64gc-unknown-none-elf -- -D warnings
```

The [runtime guest](tests/guest/runtime/README.md) exercises the production heap,
Supply lease backend and Client/Ingress with real requests, replies and Task
reclamation on an explicitly qualified CPU-only QEMU platform. This is a
component test, not the complete System or a Toolchain User Unit.
The [Loader guest](tests/guest/loader/README.md) separately creates independent
ordinary User Teams from a native ELF, with private DATA/BSS, cached RX, fresh
Task request journals, cancellation and explicit retirement on that same heap.
Its explicit test-platform feature qualifies all-Hart instruction visibility
for explicitly requested multi-Hart QEMU invocations.

The selected Kernel supports bounded all-Hart CPU data completion on the
qualified coherent-RAM fixture. Ordinary runtime CI checks one Hart.
Executable SMP requires explicit all-Hart instruction qualification; the Loader
CI exercises one Hart in both profiles. Four-Hart SMP qualification remains
an explicit non-CI target run; local-only qualification still rejects SMP. The unqualified normal Vanllia platform still rejects native
Publish. Neither boundary may be bypassed by inventing completion proof.
The [entry guest](tests/guest/entry/README.md) also exercises the production
entry, native DTB/initrd projections and internal-service bootstrap. The broad
Client guest still needs migration and must not inherit older runtime evidence.

Full Server build and dependency checks:

```sh
cargo build --locked -p system-server --bin system --target riscv64gc-unknown-none-elf
cargo build --locked -p system-server --bin system --release --target riscv64gc-unknown-none-elf
cargo metadata --locked --format-version 1
cargo tree --locked -p system-server -e normal,build --target riscv64gc-unknown-none-elf
```

Host tests, target compilation, QEMU execution and hardware execution are
separate evidence. Full System + Toolchain ordinary-Unit lifecycle acceptance
belongs to Vanllia and has not passed on this revision.

The [production application scenarios](tests/guest/system/README.md) boot the
complete release Server with an empty catalog, two transient native User Teams
with namespace/Supply installation, and a rejected resource installation with
explicit retirement. This is stronger than the separate component fixtures,
but the User ELF is not a Toolchain std Unit. The initial Task now switches to
a separate 1 MiB Capsule-owned stack before Rust, preserving the Kernel word
arguments and an unmapped lower guard. This fixes the observed 64 KiB bootstrap
stack overflow. CI runs both System profiles against its recorded release
Kernel. The application runner also requires Kernel's admitted initial-Task
exit, not only a Complete event. Heap backing retires through whole-Reservation
Release, including explicit retained Busy continuation. Fatal exits stop owned
same-Team workers and observe reclamation before releasing the root's backing;
normal completion uses Task scope. Bootstrap's five-second inactivity deadline
renews only on actual acquired resources, acknowledged phases or completed
worker initialization steps. Historical SMP/debug-Kernel verification is recorded in
the application scenario and System #13, separately from provider acceptance;
ordinary CI no longer repeats the multi-Hart matrix. Public S1/S2 lifecycle
contracts and S3/V0 are not accepted on this revision.

# System

System boots and supervises Vanllia Units. It owns execution, identity,
publication and retirement; hardware enumeration and driver policy belong to Hub.
The host Vanllia CLI selects Unit records and images for the boot archive.

This is the ongoing migration in [System #13](https://github.com/shuyra-arche/system/issues/13),
continuing [Draft PR #12](https://github.com/shuyra-arche/system/pull/12).
[Vanllia #39](https://github.com/shuyra-arche/vanllia/issues/39) owns Track S
progress and assembled-system acceptance. **The complete Server still does not
build. S0–S3 are not complete.**

## Code map

| Entry | Responsibility |
| --- | --- |
| `api/` | Protocol v4 codecs and typed service contracts |
| `client/` | Task-local clients and retained reply cleanup |
| `server/src/protocol/` | Authenticated ingress before service effects |
| `server/src/runtime/` | Initial-Task heap and native memory result ownership |
| `server/src/system/` | Services, declarations and supervisor migration |
| `support/` | Borrowed boot archive, typed ledger and bounded platform facts |
| `tests/` | Host contracts and explicit target component fixtures |

Production packages keep `no_std`. Test code belongs under `tests/`.
Read [AGENTS.md](AGENTS.md), [CONTRIBUTING.md](CONTRIBUTING.md) and
[PROVENANCE.md](PROVENANCE.md) before changing this repository.

## Selected native providers

| Provider | Exact revision |
| --- | --- |
| Kernel API/Client | `0ee104ddf94ca74a18135bf216b888a2437bd6f8` (ABI/Ledger 11/11, Capsule 2) |
| Twine | `80158211d8232c789c4609a3ea490bd16bbdac7b` |
| Weave | `7a7d67fb3f190aeb43049d051b0972bcd53213bc` |
| Unit declarations | `aaa2b58c1ef0b40428dcc71d1321736156551109` (schema 2, runtime disabled) |

Kernel [#180](https://github.com/shuyra-arche/kernel/pull/180) and Twine
[#9](https://github.com/shuyra-arche/twine/pull/9) are published candidate pins;
they must land before this consumer. The test platform selects matching Vanllia
[#40](https://github.com/shuyra-arche/vanllia/pull/40). No new release is implied.

API/Client 0.4 and protocol v4 reject v2/v3 envelopes and retired Share Open/Shut
rights. A client or ingress receives an explicit shared memory context backed by
its actual Internal Nest. Its allocator must also serve the Task's other native
memory users. See [Client lifecycle](client/README.md).

These pins describe the migrated components. The complete workspace still has
legacy Env/Wire/Mold/IPC/Resource/Execution dependencies, Cargo replacements and
a System API self Git pin. Do not mistake a clean component tree for S0's clean
production graph. Preserve the existing Identity/Operator/Loader/Supply work
while migrating the remaining Control/Assembly and application callers.

## Verification

Use `rust-toolchain.toml` (`nightly-2026-11-13`, RV64 target, Rust source,
Rustfmt and Clippy). From this repository root:

```sh
cargo test --locked -p system-api -p system-client -p system-support --tests --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path tests/identity-state/Cargo.toml --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path tests/operator-policy/Cargo.toml --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path tests/declarations/Cargo.toml --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path tests/loader-image/Cargo.toml --target x86_64-unknown-linux-gnu
cargo clippy --locked -p system-client -p system-support --lib --target riscv64gc-unknown-none-elf -- -D warnings
```

The [runtime guest](tests/guest/runtime/README.md) exercises the production heap,
Supply lease backend and Client/Ingress with real requests, replies and Task
reclamation on an explicitly qualified CPU-only QEMU platform. This is a
component test, not the complete System or a Toolchain User Unit.
The [Loader guest](tests/guest/loader/README.md) separately creates independent
ordinary User Teams from a native ELF, with private DATA/BSS, cached RX, fresh
Task request journals, cancellation and explicit retirement on that same heap.
Its explicit test-platform feature qualifies local instruction visibility only
for the paired one-Hart QEMU invocation.

The selected Kernel's native memory backend rejects multiple Harts at
qualification: remote translation acknowledgements do not prove remote CPU data
accesses have drained. The unqualified normal Vanllia platform also rejects
native Publish. Neither boundary may be bypassed by inventing completion proof.
The [entry guest](tests/guest/entry/README.md) also exercises the production
entry, native DTB/initrd projections and internal-service bootstrap. The broad
Client guest still needs migration and must not inherit older runtime evidence.

The full Server gates remain reproducible failing checks until integration is
ported:

```sh
cargo build --locked -p system-server --bin system --target riscv64gc-unknown-none-elf
cargo build --locked -p system-server --bin system --release --target riscv64gc-unknown-none-elf
cargo metadata --locked --format-version 1
cargo tree --locked -p system-server -e normal,build --target riscv64gc-unknown-none-elf
```

Host tests, target compilation, QEMU execution and hardware execution are
separate evidence. Full System + Toolchain ordinary-Unit lifecycle acceptance
belongs to Vanllia and has not passed on this revision.

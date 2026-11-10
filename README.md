# System

System boots and supervises Vanllia Units. It owns execution, identity,
namespace, readiness and retirement. The host Vanllia CLI selects Unit records
and images for initrd; System validates the bundle and instantiates startup or
requested Units. Deployment remains a host-side construction.

## Code map

| Entry | Responsibility |
| --- | --- |
| `api/` | Public System protocol types and contracts |
| `client/` | Typed clients for System protocols |
| `server/` | Runtime state, supervision and the `system` executable |
| `server/unit.toml` | System's Unit declaration |
| `support/` | Borrowed initrd catalog, boot facts and shared helpers |
| `tests/` | Independent host fixtures and structural checks |

Operator and Identity bootstrap are internal System threads. Account, drivers
and product selection remain outside System. `support/` must not acquire a
server/client dependency. See [runtime contracts](docs/runtime-contracts.md).

## Build and verify

Use `rust-toolchain.toml` (`nightly-2026-09-04`, RV64 target and Rust source).
Run from this repository root; examples use a Linux x86-64 host.

```sh
cargo build --locked -p system-server --bin system --target riscv64gc-unknown-none-elf
cargo test --locked -p system-api --tests --target x86_64-unknown-linux-gnu
cargo test --locked -p system-server --lib --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path tests/dynamic-units/Cargo.toml --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path tests/instance-events/Cargo.toml --target x86_64-unknown-linux-gnu
cargo test --locked --manifest-path tests/loader-image/Cargo.toml --target x86_64-unknown-linux-gnu
```

These are separate checks: host fixtures do not establish boot, SMP, driver
recovery or physical reclamation. Product image construction and guest testing
belong to [Vanllia](https://github.com/shuyra-arche/vanllia).

## Current limitations

The pinned nightly needs fresh verification: previous executable checks used
`nightly-2026-10-09`. The old `test-support` scenarios still use declarations and
Unit shapes from before dynamic selection and require repair; do not treat all
historical fixtures as a passing current suite. Early owner death can prevent a
requested Unit's Retired frame from being published; absent receipts must fail
acceptance rather than imply reclamation.

Root Cargo `[replace]` entries select forwarding SDK revisions for specific old
pins. They apply only to this workspace root; consumers must unify their own
Env/Wire/Mold and canonical Kernel API sources. The public Git System API pin is
also distinct from the local API workspace member. Keep lockfiles and actual
resolved dependency identity aligned when updating either.

See [contribution rules](CONTRIBUTING.md) and [source attribution](PROVENANCE.md).

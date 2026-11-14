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

The protocol v3 migration branch has API and Client on Kernel ABI 10, Twine and
Weave. `server/src/protocol` supplies the new ingress and is exercised directly
by `tests/guest/client`. The legacy Server application still uses API v1 and
removed Client facades; its full build currently fails until those services
are ported. This branch remains draft. See [Client lifecycle](client/README.md)
and [ingress boundaries](server/src/protocol/README.md).

The native migration checks below use the explicitly recorded nightly compiler in the PR evidence:

```sh
cargo +nightly test --locked -p system-api --tests --target x86_64-unknown-linux-gnu
cargo +nightly clippy --locked -p system-client --all-targets --target riscv64gc-unknown-none-elf -- -D warnings
cargo +nightly test --locked --manifest-path tests/identity-state/Cargo.toml --target x86_64-unknown-linux-gnu
python3 tests/guest/client/run.py /path/to/shuyra
```

The guest uses a real ABI 10 Kernel, one and four Harts and the actual production
SDK/ingress. It does not launch the assembled System policy/boot application.
The production Identity book, request dispatcher, multi-endpoint Task loop and
revision notifications are ported and tested through this ingress. Independent
host tests are documented in [tests/identity-state](tests/identity-state/README.md),
and the [service lifecycle](server/src/system/identity/service/README.md) records
its native ownership and cleanup rules. Supervisor wiring and the other service
loops still need migration.

The production Operator namespace, mutation journal, native grant lifecycle and
watch loop are also ported. Its [ownership contract](server/src/system/operator/runtime/README.md)
distinguishes Unbind retention from authority withdrawal. The guest runs real
controller/publisher/Operator Tasks and verifies namespace operations, scoped
source checks, reply-failure replay, consumer grants and cleanup. Legacy Operator
session, handshake and Schedule paths are removed; supervisor callers still need
their native bootstrap migration.

Native [internal-service bootstrap](server/src/system/app/boot/README.md) now
creates both Tasks, explicitly grants their startup resources and connects
Root/Wired without the old Schedule/closure bootstrap. Its production worker
owner keeps execution exit, Kernel reclamation and record deletion separate.
The guest verifies normal/partial startup, cancellation, both roles and cleanup.
The [support package](support/README.md) is on Kernel 10 and the pure Unit
bundle, including typed boot accounts and explicit Share/Sign dispatch pairs.
The assembled app still has legacy supervisor/service callers; removal of those
callers remain outstanding. The native entry/linker and growing, reclaiming heap
are now implemented and tested separately with the real service bootstrap;
complete supervisor integration is still required.

The [native Loader](server/src/system/loader/README.md) now constructs real User
Teams through generation-qualified Cell associations and publications. A
receiving-Task initializer installs private RW mappings under v10, then jumps to
the image with copied arguments. RX cache eviction retains live Unit sources;
DATA/BSS isolation, partial cancellation, native exit and reclamation are tested
on one/four Harts. RPC admission and the remaining application wiring are still
being migrated. See the [guest workflow](tests/guest/client/README.md).

Cross-Team delegation now uses Kernel #140's fixed registration quotas. The
component guest exercises S -> Dormant U initial grants and U -> S request/reply,
with real sender validation, rejected forged routes, no execution-control upgrade,
and explicit cleanup. See [the guest workflow](tests/guest/client/README.md).
Protocol v3 and API/Client 0.3 use Kernel v10's nine-word typed Limits; v2 frames
are rejected. Rebuild all consumers and Capsule/Ledger inputs together.

Use `rust-toolchain.toml` (`nightly-2026-11-13`, RV64 target and Rust source).
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

The support package passes its host tests and RV64 check on the pinned nightly.
The assembled Server still cannot build; its runtime heap still uses retired ABI 9 Cell construction calls, and Control/Assembly, the remaining service
integration and the mixed shipping graph require migration. The native entry
is verified with an explicit contract fixture, not the complete application. The old `test-support` scenarios still use declarations and
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

The [native entry/heap](server/src/runtime/README.md) keeps heap backing in the
initial Task, so internal worker exit cannot invalidate live allocations. The
[entry contract](tests/guest/entry/README.md) boots the exact production entry,
ledger/DTB decoder and actual Operator/Identity startup on that heap. The
[runtime guest](tests/guest/runtime/README.md) verifies concurrent allocations,
worker exit, page reclamation and native Supply lease/window ownership on one
and four Harts. Neither fixture is the complete application.

System now owns a [schema-2 interpreter](server/src/system/declaration/README.md)
using the merged pure Unit provider. Named rights are translated to checked
Kernel limits; startup selection and cross-record policy are validated without
old permission masks. The [Supply lease backend](server/src/system/supply/README.md)
uses private native roots and retryable withdrawal. Legacy Control/Assembly,
Supply RPC/provider collections and supervisor callers still need integration.

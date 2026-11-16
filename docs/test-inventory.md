# Test and support migration inventory

Historical snapshot: System `470e97f38aa904c3702b665c0d225809b9f0d16e`
(ABI/Ledger 13/13), shared standards `.github`
`7967d812c6c9d40fde6a174483073b788af3e3cc`, taken on 2026-11-15. Its
assertion-port queue remains relevant to [System #13](https://github.com/shuyra-arche/system/issues/13),
but its counts and path/status descriptions are not the current #28 M0
baseline. See the [native test migration baseline](native-test-baseline.md)
for the fixed System `332d9d2e15f6bc4e1e8d94e4db2591b1a53ae613` M0 snapshot
on 2026-11-16 and its separately dated current migration status.

## Current #28 migration status

This file preserves the #13 package/count snapshot below; do not read those
tables as the current checkout inventory. Since that snapshot, the active Host
tests have moved to owner-aligned suites under `server/tests/host/`, and the
native scenario tree has moved under `server/tests/target/`. The fixed M0 set of
114 Host functions remains the migration baseline. At System
`34b8aa9349795a7b489f68222a1812516fb2bada`, the active Host suites contain 130
functions: 92 Server owner tests plus 38 API, Client and Support tests. The 16
additional assertions were added during owner migration and regression work.
The candidate Rust runner has 11 unit tests and the App pack checker has 4
tests; those counts are separate from the Host count.

Only two historical packages have been removed after their current-owner
assertions were added: `dynamic-units` (App declarations) and
`capability-import` (Operator Watch). The remaining historical packages still
contain unported stable safety assertions or native/integration-only gaps.
Their source and gaps remain mapped in the [service migration map](service-test-migration.md),
[Control migration map](control-test-migration.md), and the [fixed baseline](native-test-baseline.md).
No statement here declares M1, native parity, CI transition or clean-checkout
acceptance complete.

## Scope and evidence

The baseline contains **44 manifests below root `tests/`**: five active Host
packages, nine guest/build packages and thirty historical packages. The four
workspace packages (`api`, `client`, `server`, `support`) are separate. The five
Python `run.py` entry points are runtime, entry, client, loader and system.
The tables account for every baseline manifest. Paths are relative to `tests/`
unless stated otherwise; a row denotes a `Cargo.toml`, not one test function.

Current commands are in the root README and `native.yml`. Host tests prove
policy and scripted owner transitions; compiling for RV64 proves neither native
traps nor QEMU behavior. Only the current qualified-platform runs can establish
the guest claims. Hardware and full System + Toolchain lifecycle remain separate.

`Moved` means the original assertions are retained in the new explicit Server
test targets. `Keep / port` means an active test still awaits its owner migration.
`Historical / port` means the old test expresses safety assertions worth mapping
to the current owner, but is not current coverage. Those packages were not run:
they retain retired Env/Wire/ECS contracts, source paths or SDK declarations.
Restoring that production dependency graph to make them pass is not a migration.
`Historical / retire shape` applies to obsolete syntactic rules; semantic
assertions in the same package still need behavioral counterparts.

## Active Host packages

| Baseline manifest | Decision and current location | Owner / retained assertion |
| --- | --- | --- |
| declarations | Moved → `server/tests/app/declarations.rs` (6) | Validated schema-2 intent, rights, startup ordering, authority domains |
| loader-image | Moved → `server/tests/loader/image.rs` (3) | Loader ELF admission and segment/entry validation |
| native-memory | Moved → `server/tests/runtime/mod.rs` (6) | Scripted delivery, retained versions, mutation identity and explicit continuation; no native trap claim |
| identity-state | Moved → `server/tests/identity/` (18) | Identity book, revision publication and decoded dispatch; suite remains independently runnable |
| operator-policy | Moved → `server/tests/operator/` (21) | Operator grants, namespace roots, policy, runtime state and dispatch; suite remains independently runnable |
| control | Current → `server/tests/control/` (15) | Construction/install journals, receipts and Program lifecycle decisions |
| supply | Current → `server/tests/supply/` (8) | External registration/withdrawal ownership and caller-scoped installation selection |

The eight Server Journal/Ledger tests formerly embedded in `api/tests/control.rs`
now belong to `server/tests/control/construction.rs`. API tests retain bounded
codec and nonzero-ID assertions. Four Program tests cover retained selection,
resident/transient completion, physical-retirement dependence and supervision
policy. The seven Server owner suites contain 77 current Host tests and are
registered explicitly with `[[test]]` targets and
directory `mod.rs` entry points. Identity and Operator tests are now part of the
same Server Host package and remain focused Cargo targets. Their prior inline
test-module source paths describe the #13 snapshot. At that snapshot,
`cfg(test)` hooks remained where assertions inspected private owner state. In
the later #28 migration these hooks and the test-only Identity `root()` helper
were removed; the monotonic revision exhaustion assertion remains in the
Identity owner suite. See the current status section above.

## Guest and composition packages

| Baseline manifest | Decision / destination | Boundary and limitation |
| --- | --- | --- |
| guest/runtime | Keep → eventual `tests/target/runtime/` | Native heap, Supply and Client/Ingress component; runtime runner |
| guest/runtime/platform | Keep → eventual `tests/target/platform/` | Exact Kernel + platform pins; qualified CPU fixture, not normal Vanllia |
| guest/entry | Keep → eventual `tests/target/entry/` | Production entry, DTB/initrd projections and internal bootstrap |
| guest/entry/pack | Keep with entry | Schema-2 archive producer, not an independent execution proof |
| guest/client | Historical broad guest / port; preserve capsule helper | README explicitly withholds broad Client acceptance; its Python capsule helper remains used by current runners |
| guest/client/image | Keep with Loader/System target scenarios | Ordinary User ELF fixture; no acceptance by compilation alone |
| guest/loader | Keep → eventual `tests/target/loader/` | Independent User Teams, private data, RX cache, cancellation, retirement; optional actual Toolchain std-waits is separate evidence |
| guest/handoff | Keep with System target scenarios | Independent-Team authority, selected owner, cancellation/death and UnitRoot/Supply observations |
| guest/system/pack | Keep with System target scenarios | Archive construction and serial assertion helper for the full production Server runner |

`guest/system/run.py` is a runner without its own manifest. Preserve all seven
handoff scenarios across System/Kernel debug/release and 1/2/4 Harts (84 cases),
the application matrix (36), Loader (6) and runtime/entry component campaigns.
Do not move these paths without updating artifact manifests, hashing, cache keys,
the shared-platform producer/consumers and every runner import together. ABI 11
ELFs from older acceptance cannot qualify the ABI 13 graph.

## Historical packages: assertion destinations

These rows are a porting queue, not claims that a similarly named basic scenario
already covers every old fault or pressure campaign.

| Baseline manifest | Decision | Current semantic owner / assertion to port |
| --- | --- | --- |
| bootstrap-carrier | Historical / port | App boot + construction: partial delivery retains roots, retry identity, release order |
| capability-import | Moved / removed | `server/tests/operator/watch/`: forged imports preserve live subscriptions, caller-local retirement and same-path rejoin; Kernel/transport effects remain scripted |
| control-instance | Historical / port | Instance + Operation: lifecycle effects, partial failure and compensation |
| control-lifetime | Historical / port | Instance: owner exit, explicit retirement and retained resource roots |
| control-publisher | Historical / port | Program + Operation: publication CAS and historical receipts |
| control-query | Historical / port | Control observation: caller scope, cursors and unknown identities |
| control-rpc | Historical / port | Control ingress: grant authorization, decode and response correlation |
| daemon-control | Historical / port | Program + Instance: resident demand, recovery and restart ordering |
| dynamic-units | Moved / removed | `server/tests/app/declarations.rs`: shared-image Requested Units stay unstarted, missing/cyclic references and image-kind admission reject invalid input |
| guard-deployment | Historical / port | Control construction: guarded authority and deployment policy |
| identity | Historical / port | Identity authority: principal, coalition and membership checks |
| identity-publication | Historical / port | Identity revision publisher: coalesced publication and cache ordering |
| identity-rpc | Historical / port | Identity ingress: authority source, decode and bounded replies |
| instance-events | Historical / port | Program/Instance observations: event identity and retained execution history |
| lease-window | Historical / port | Supply leases: rights/window intersection and retirement |
| loader-api | Historical / port | Current API + construction: bounded input and immutable request identity |
| loader-client | Historical / port | Client + construction Ledger: request correlation, retry and cleanup |
| namespace-installation | Historical / port | Instance Installation: retained grants and publication compensation |
| namespace-layout | Historical / port | Global Layout + Operator: shared/instance placement and authority |
| namespace-retirement | Historical / port | Instance Installation: revoke/unbind/unpart acknowledgments before release |
| operator-api | Historical / port | Current Operator API: bounded frames and typed rights |
| operator-client | Historical / port | Operator Client: source authentication, revisions and cleanup |
| operator-connection | Historical / port | Operator runtime: endpoint ownership and connection lifetime |
| operator-handoff | Historical / port | Operator grant owner: holder/source restrictions and retained handoff state |
| operator-watch | Historical / port | Operator Watch: notification ordering, cursors and retirement |
| retirement-boundary | Historical / port | Instance Gate: actual provider settlement before Stop; native acknowledgment required |
| shutdown-policy | Historical / port | Control close ordering: dependent retirement before provider stop |
| startup-authority | Historical / port | App boot + Installation: verified creator and delivered startup authority |
| supply | Historical / port | Supply registry/leases: acquisition, rollback, release and device retirement |
| system-shape | Historical / retire shape; port semantic assertions | Retire universal three-parameter and old ECS/injected-hook/path rules. Port retirement order, same-Team pause/resume, dependency direction and owner privacy as behavior or dependency checks |

This #13 snapshot's manifest count is historical. In the later #28 migration,
`dynamic-units` and `capability-import` were removed only after their stable
assertions gained current owner tests. Other historical sources remain because
their safety assertions have not all been ported or accepted as obsolete.

## `system-support` consumers

Removal is **not ready**. Actual package manifests and imports were inspected;
the package must stay until production and test consumers migrate.

| Source revision | Consumer | Selection / use | Required next step |
| --- | --- | --- | --- |
| System `470e97f38aa904c3702b665c0d225809b9f0d16e` | `server/Cargo.toml`, `server/src/lib.rs`, `server/src/boot/mod.rs`, App boot launch | Local `../support`; debug/support reexports, boot Accounts/Catalog/Dispatch/Resource | Assign each primitive to its runtime/boot owner and migrate imports before deleting package |
| Same System revision | `tests/guest/entry/Cargo.toml` and entry source; `support/tests/` | Real entry boot projections and boot contract tests | Migrate tests and fixtures with the owning primitives; retain native entry acceptance |
| Driver `aa84bdedcf713c4bfce85e1c2ab65acedf1c4409` | `shared/Cargo.toml`, `shared/src/context/context.rs` | Direct System pin `5a792ff2d39901a970925b1a6d63206adf6c16d8`; actual `system_support::debug` import | Publish replacement owner, migrate consumer, then verify consumer graph |
| Same Driver revision | Root `Cargo.toml`, `tests/shared/Cargo.toml` and lockfiles | Workspace replacements select System `8eea7c9039cac0e6ada4a59c612dc6c98ef9bc45` for `system-support@0.1.0` | Update both production and independent Host composition; preserve fetchable old pins |

Discovery used connected organization code search for both `system-support`
and `system_support`, followed by exact-revision reads of the Driver manifests
and importing source. Search returned no additional default-branch matches in
the scoped Hub/Vanllia queries. This is an indexed default-branch inventory,
not proof that every branch, unpublished workspace or downstream organization
has zero consumers. Frozen historical fixture imports do not count as current
production consumers, and do not justify restoring retired production edges.

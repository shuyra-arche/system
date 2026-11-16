# Native test migration baseline

Baseline: System `332d9d2e15f6bc4e1e8d94e4db2591b1a53ae613` (2026-11-16,
after #27), shared standards `.github`
`7967d812c6c9d40fde6a174483073b788af3e3cc`. This records M0 for
[System #28](https://github.com/shuyra-arche/system/issues/28). It is a map of
the current checks and their migration destinations, not a new acceptance
claim. Host, target compilation, QEMU execution, hardware, and full
System/Toolchain acceptance remain separate evidence.

The older [test inventory](test-inventory.md) is a historical #13 migration
snapshot dated 2026-11-15 (System `470e97f38aa904c3702b665c0d225809b9f0d16e`).
Its historical assertion queue remains useful, but its counts and current-path
statements are not the #28 baseline below.

The last pre-merge candidate `b6ca9d4fa3b8e70e3e9b15abd745077518093cc0`
passed [run 43799340357](https://github.com/shuyra-arche/system/actions/runs/43799340357).
The baseline merge commit itself did **not** pass every job:
[run 43802055998](https://github.com/shuyra-arche/system/actions/runs/43802055998)
passed 22 of 23. `Handoff / supply / Kernel debug / System debug / Harts 1`
failed with `FAIL handoff`, `TimedOut` at `main.rs:183:29`, after a stuck
`clone_view` diagnostic. This is a behavioral failure, not the positively
identified console-loss inconclusive case. The migration must retain its
failure and deadline criteria; the matrix sizes below describe protected
coverage, not a claim that this merge revision is fully green.

## Fixed inputs and verification rules

The root README, manifests, lockfiles, platform fixture, runner code, and
`.github/workflows/native.yml` currently select:

| Input | Selected value |
| --- | --- |
| Kernel API / Client | `789ffd19b5359f3de172fb9aad8a61f6a7113a64`; ABI 13, Ledger 13, Capsule 2 |
| System API / Client | version 0.4; protocol v4 |
| Twine | `932454c07bd2673ac7d6d9612d36182aae71e8b2` |
| Weave | `7a7d67fb3f190aeb43049d051b0972bcd53213bc` |
| Unit declarations | `aaa2b58c1ef0b40428dcc71d1321736156551109`; schema 2, runtime disabled |
| QEMU platform provider | Vanllia `0eb086a2416eee8921416b12bf94819b30b6bb1f` with the same Kernel revision |
| Rust | `nightly-2026-11-13`; target `riscv64gc-unknown-none-elf` |
| Target flags | `-Crelocation-model=static -Cforce-frame-pointers=yes -Ccode-model=medium -Dwarnings` |
| QEMU system | `virt`, CPU `rv64`, 128 MiB, no network device, OpenSBI firmware |

CI builds and records three shared platform ELFs: default-feature release for
runtime/entry, and `instructions` feature debug and release for Loader,
application, and handoff. All-hart executable publication qualification is
limited to the paired coherent QEMU CPU fixture; no External requester,
hotplug, unmanaged executable alias, device retirement, or physical hardware
is covered. The normal platform remains data-only and fails closed for
executable publication.

Guest success requires all scenario-specific expected markers and a zero QEMU
exit, plus the scenario's typed event and retirement assertions. A `PASS` line
alone is not a pass. Application and handoff checkers require Kernel's admitted
initial-System Task exit with the expected reason/note after typed `Complete`;
they also reject guest faults/panics and require task-exit/retirement ordering.
Handoff's sole retry is one fresh VM after *all* behavioral assertions passed
when the exact root-exit line is absent and Kernel reports positive
`console-lost`. The first attempt remains an inconclusive failed receipt; a
second inconclusive attempt fails. Timeouts, QEMU nonzero status, guest faults,
other missing assertions, or failed retirement are failures and are never
accepted or retried. Application has no inconclusive retry. Runtime, entry,
and Loader have no retry. Evidence is written per invocation under generated
`target/` paths and uploaded by CI; raw logs are not a permanent documentation
artifact.

| Scenario group | Exact profile / Hart selection | Budget and required terminal evidence |
| --- | --- | --- |
| Runtime component | release guest, default platform; Harts 1 and 4 | 120 s per QEMU. Four runtime/Supply/window/RPC marker families, zero exit; native heap owner and reclaimed worker contracts. |
| Entry normal | release fixture, default platform; Hart 1 | 90 s. Requires entry `words=4 ledger=13 catalog=2 dtb=1 heap=1`, services `heap=production roles=2 root=1 wired=1 retire=1`, boot-data `native=1 readonly=1 content=1 padding=1 retire=1` markers; root exit `reason=0x0 note: system: complete`; all tasks exited. |
| Entry abort | release fixture with `abort`; Harts 1 and 4 | 90 s each. Requires two-live-worker marker and root exit `reason=0x2a note: system: injected entry failure`; both workers reclaimed and all tasks exited. |
| Entry stalled bootstrap | release fixture with `stall`; Hart 1 | 90 s total; unchanged five-second inactivity deadline despite polling. Requires stall marker and root exit `reason=0x2b note: system: stalled entry bootstrap`; worker reclaimed and all tasks exited. |
| Loader | Kernel debug and release; Loader debug/release paired to Kernel profile; ordinary User ELF always release; Harts 1, 2, 4 | 120 s per QEMU. Requires `PASS loader-elf native=1 user=1 arguments=1 private=1 cache=1 eviction=1 cancel=1 running-stop=1 journal=1 reclaim=1`, no `FAIL`, zero QEMU exit, Kernel all-tasks-exited shutdown. Six cases. CI uses `instructions` platform. |
| Application | Kernel debug/release × System debug/release × Harts 1, 2, 4; User ELF release | 90 s per VM. One-User rejected installation requires Complete reason `4` and admitted root exit `reason=0x4 note: system: supervision failed`; two-User completion requires reason `0` and root exit `reason=0x0 note: system: complete`. Typed Task exit before retirement, cleanup, expected marker/events and all-tasks-exited are also required. 36 cases. |
| Handoff | Kernel debug/release × System debug/release × Harts 1, 2, 4; User ELFs release with LTO and one codegen unit; default/provider/install features selected by scenario | Release Kernel budget 60 s per role, 90 s per VM; debug Kernel budget 120 s per role, 180 s per VM. Eleven scenarios × four profile pairs × three Hart counts = 132 VMs, plus 3 release/release `supply-contend` VMs. Typed scenario events, Task exit then retire, independent Teams, Complete reason `0`, and admitted root exit `reason=0x0 note: system: complete` required. |

The runtime component runner requires all four exact serial markers:
`PASS system-runtime heap=1 growing=1 shared=1 owner=1 worker-exit=1 reclaim=1 contention=1`,
`PASS supply-native rights=1 scope=1 replay=1 cancel=1 reorder=1 withdrawal=1 cleanup=1`,
`PASS supply-window relative=1 narrowed=1 once=1 cleanup=1`, and
`PASS system-rpc abi=13 protocol=4 context=1 sender=1 reply=1 cleanup=1 worker-reclaimed=1`.
Entry abort additionally requires
`EXPECTED entry-abort workers=2 backing=live`; stall requires
`EXPECTED entry-stall timed-out=1`. In both modes the runner separately checks
the exact root exit above, rejects panics/faults, and requires all tasks exited.
Entry normal requires the exact markers represented in the table and the
production root exit; it is not accepted from marker text alone.

The handoff checker in `tests/guest/system/pack/handoff.rs` also requires the
exact scenario marker: `handoff` → `PASS handoff recovery=1 instance=1 roots=1 cas=1 rpc=1 supply=1 revoke=1 stop=1 retire=1`;
`auth` → `PASS handoff create=1 replay=1 conflict=1 root-grants=1 stale=1`;
`cancel` → `PASS handoff lost-reply=1 pending-cancel=1`;
`death` → `PASS handoff owner-death accepted=1`;
`owner` → `PASS handoff selected-owner creator-exit=1 lifecycle=1 root=1`;
`owner-cancel` → `PASS handoff selected-cancel requester=1 cleanup=1`;
`owner-death` → `PASS handoff selected-death receipt=1 isolation=1`;
`supply` → `PASS handoff provider=1 register=1 installed=1 replay=1 revoke=1 ancestor=1 rebind=1`;
`supply-auth` → `PASS handoff supply-auth=1 replay=1 tombstone=1 rights=1 retained=1`;
`install` → `PASS handoff post-prepared=1 target=1 replay=1 cancel=1 isolation=1 withdraw=1 retire=1`;
`install-loss` → `PASS handoff install-loss=1 start-gate=1 cleanup=1`.
The `supply-contend` check additionally requires `PASS supply-contention pairs=`.
These text markers accompany the typed checker; they do not replace it.

The handoff default scenarios are `handoff`, `auth`, `cancel`, `death`,
`owner`, `owner-cancel`, `owner-death`, `supply`, `supply-auth`, `install`,
and `install-loss`. The first seven use the default User fixture; `supply`,
`supply-auth`, and `supply-contend` use `provider`; `install` and
`install-loss` use `install` (which enables provider). Every invocation
preserves the exact selected System, User, Kernel, firmware, Cargo locks,
archive and image hashes, profiles, Hart count, command, serial output and JSON
receipt. Retry does not overwrite first-attempt evidence.

## Host coverage: current entry to planned owner

At the fixed M0 snapshot, there were **114 current Host `#[test]` functions**, counted from Rust source
under the active API, Client, Support, Server, Identity-state and Operator-policy
test roots. This is the M0 source count, not a claim that tests were run in that
snapshot. The baseline CI invoked them through the root README and `native.yml`.

| Current entry and count | Semantic owner / contract | Planned entry after M1 |
| --- | --- | --- |
| `api/tests/{protocol,identity,shape,control}.rs` (31) | API protocol framing/codecs, bounded IDs, identity codecs, public data contract. Server Journal/Ledger assertions remain Server-owned; API keeps only codec and ID contract. | API test package stays under `api/tests/`; move Server behavior to `server/tests/control/`. |
| `client/tests/context.rs` (2) | Client-owned task-local context and retained reply cleanup. | `client/tests/` (retain local package boundary). |
| `support/tests/boot.rs` (4) | Support-owned boot projection and typed boot facts. | `support/tests/` while Support remains a production owner. |
| `server/tests/supervision/control.rs` (8) | Control Journal/Ledger transitions, receipts and operation observation. | `server/tests/control/`. |
| `server/tests/supervision/declarations.rs` (6) | Declaration validation, schema-2 intent, rights and startup ordering. | `server/tests/app/` or declaration/App owner entry selected during M1. |
| `server/tests/supervision/image.rs` (3) | Loader image admission, ELF segment and entry validation. | `server/tests/loader/`. |
| `server/tests/supervision/installations.rs` (3) | Prepared installation admission, grant delivery and installation receipts. | `server/tests/control/` or the final Instance/Installation owner entry. |
| `server/tests/supervision/programs.rs` (4) | Program selection/publication, resident/transient completion, retirement dependence and supervision policy. | `server/tests/control/` or the final Program owner entry. |
| `server/tests/runtime/mod.rs` (6) | Native memory result delivery, retained versions, mutation identity and explicit continuation; scripted Host effects do not prove native traps. | `server/tests/runtime/`. |
| `server/tests/organization/supply.rs` (8) | Supply registration and lease behavior. | `server/tests/supply/`. |
| `tests/identity-state/{book,dispatch,revision}.rs` (18) | Identity book and revisions; publication/dispatch authority and exhaustion. | Merge into `server/tests/identity/` after behavior is tested through owner transitions. |
| `tests/operator-policy/{authority,namespace,policy,roots}.rs` (21) | Operator grant authority, namespace roots, policy, dispatch, and root behavior. | Merge into `server/tests/operator/` after behavior is tested through owner transitions. |

CI Host invocation groups are: API/Client/Support integration tests;
`server/tests/Cargo.toml`; and separate `tests/identity-state` and
`tests/operator-policy` packages. The 114 count was checked against the
current test attributes rather than copied from the older inventory or prior
CI notes.

At M0, the Identity book/revision and Operator authority/tree tests depended on
production `#[cfg(test)]` hooks, including a test-only Identity `root()` helper
and private exhaustion checks. In the current tree those hooks and the helper
have been removed; `server/src/` contains no `#[cfg(test)]` test hooks. The
owner suites reach private production behavior through test-local source
facades instead of adding production test counters or public API.

## Native scenario mapping

| Current entry | Semantic owner and protected contract | Planned entry | Current evidence and status |
| --- | --- | --- | --- |
| `tests/guest/runtime/{run.py,main.rs}` plus `tests/guest/runtime/platform/` | Runtime heap/memory result owner, Supply lease backend, Client/Ingress RPC: root-owned storage survives worker exit; bounded allocation/growth; lease scope, rights, replay, cancellation, withdrawal, relative narrowing, publication cleanup; protocol-v4 actual sender/reply and worker reclamation. | `server/tests/target/runtime/` with a runner interface in `server/tests/runner/`. | CI invokes Harts 1 and 4 on default-feature release platform. 120 s/QEMU; all four marker groups, QEMU zero exit; no SMP instruction-fetch claim. Active component evidence, not complete Server acceptance. |
| `tests/guest/entry/{run.py,lib.rs,pack/}` | Production entry, boot decoding/projection, schema-2 interpretation, heap and internal-service construction; abort cleanup and bootstrap inactivity timeout. | `server/tests/target/entry/`; archive/fixture remains with its scenario owner. | Base one-Hart entry plus abort on 1/4 Harts and stall on 1 Hart: 4 total QEMU executions. 90 s each; markers/expected root exit/all-tasks-exited required. Active native component evidence. |
| `tests/guest/loader/{run.py,main.rs,fixture.rs}` and `tests/guest/client/image/` | Loader and Runtime: independent ordinary User Teams from ELF; argument/data/BSS isolation, RX reuse/cache eviction, per-Task request journal, partial cancellation, Stop/reclaim, mapping and guard retirement. | `server/tests/target/loader/`; ordinary ELF remains a dedicated fixture package under the target tree. | Kernel/Loader debug and release paired, Harts 1/2/4: 6 VMs. 120 s, `instructions` platform, marker + zero QEMU exit + Kernel all-tasks-exited. Toolchain std-waits mode is opt-in and is not part of the 6. Active Loader component evidence. |
| `tests/guest/system/{run.py,pack/}` and `tests/guest/client/image/` | Application composition (App, Loader, Runtime, Supply, Identity, Operator): empty catalog; two transient real ordinary User Teams with private memory, arguments, fresh request journals, namespace and Supply grants; rejected resource install and retirement. | `server/tests/target/app/` (scenario and checker remain System-owned); execution mechanics in `server/tests/runner/`. | Kernel × System 2×2 profiles, Harts 1/2/4, 3 scenario launches per pairing: 36 VMs. 90 s each. Current full Application gate. Not a Toolchain std Unit or full S1/S2 acceptance. |
| Same System runner with `--handoff`; `tests/guest/handoff/` | System Control/Instance/Operation, Operator UnitRoots, Supply and native lease owners: independent-Team owner and requester scope, replay/query/CAS, direct RPC, cancellation/lost reply, source withdrawal, Prepared installation/cancel, selected-owner exit/death, root/Task physical retirement. | `server/tests/target/handoff/`; consume real Server/User artifacts through `server/tests/runner/`. | Eleven scenarios × 4 profile pairs × Harts 1/2/4 = 132 VMs; separate `supply-contend` is 3 additional release/release VMs. Per-role/VM deadlines above; all assertions, typed exit/retire order, Complete and admitted root exit required. One retry only for qualified positive console-loss inconclusive. Active subset of Instance v1; no full public S1/S2 acceptance. |
| `tests/guest/client/{run.py,main.rs,*.rs}` | Broad Client, Loader, Identity/Operator and worker contracts. Its assertions cover cross-Team reply/grant, loader memory, supervisor bootstrap/worker, operator service, client timeout/late reply/backpressure/cleanup, identity actor/query/life, and wait-group behavior. | Assertion-by-assertion owners under `server/tests/target/{runtime,loader,identity,operator,entry}/`; actual Client public contract remains in Client's own tests. | Historical/broad guest: current README explicitly withholds broad Client acceptance. CI does not run this guest. Its `initrd` helper is imported by current runtime/entry/loader/system Python runners, so helper extraction/migration must accompany deletion of this runner. Do not count its PASS markers as current evidence. |

The shared platform build is a producer, not an additional behavior scenario:
one default release platform plus `instructions` debug and release. The
workflow has 23 required jobs: platform producer (1), components (1), runtime
and entry (1), Loader (2), application (4), handoff (14). The CI workflow
retains 22 artifact/cache post outputs. It records exact shared-platform
metadata and hashes; changing the runner must preserve these inputs and
provenance, not merely reproduce a printed PASS line.

The old command example in `tests/guest/runtime/README.md` still names Kernel
`684f479c45dc2d94d405b32b4ffc6334a4233fc9` and Vanllia
`58498c13f41ad64a1fc09af338802b279347c9c8`. Those are not the selected
platform dependencies at this baseline: the effective `tests/guest/runtime/platform/Cargo.toml`
and lockfile pin Kernel `789ffd19b5359f3de172fb9aad8a61f6a7113a64` and
Vanllia `0eb086a2416eee8921416b12bf94819b30b6bb1f` as listed above. Refresh
that scenario README when migrating its runner; use the manifest/lockfile and
CI-produced hashes as execution provenance.

## Historical safety assertions to port

These historical manifests are not current coverage and are not to be
revived by restoring retired Env/Wire/ECS or SDK dependency graphs. The
semantic assertion must gain a current owner test before its old package is
removed. This queue is frozen as recorded at M0; current package dispositions
are summarized below and individual maps are linked from the test inventory.
The existing older inventory contains the longer assertion-by-assertion notes;
this compact queue binds each to M1 owner work:

| Historical entry | Safety/contract assertions still requiring current-owner disposition |
| --- | --- |
| `bootstrap-carrier`, `startup-authority`, `dynamic-units`, `guard-deployment` | Partial construction/delivery retains roots; retry identity and release order; creator/startup authority; dynamic declaration and image admission; guarded deployment policy. Owner: App boot, Declaration, Construction/Control. |
| `capability-import`, `control-rpc`, `identity-rpc`, `operator-client`, `loader-api`, `loader-client` | Authenticate actual caller/source; grant kind and rights; reject malformed/bounded frames; correlate replies and preserve immutable request identity; clean partial/lost replies. Owners: Protocol/Ingress plus the relevant service or Client contract. |
| `control-instance`, `control-lifetime`, `control-publisher`, `control-query`, `instance-events`, `daemon-control`, `retirement-boundary`, `shutdown-policy` | Lifecycle effect ordering, partial failure/compensation, owner exit and retained roots, publication CAS and old receipts, scoped queries/cursors, event identity, restart/recovery ordering, provider settlement before Stop, dependency retirement before close. Owners: Control, Instance, Operation, Program and App as actual state owner. |
| `identity`, `identity-publication` | Principal/coalition/membership checks; coalesced revision publication and cache ordering. Owner: Identity. |
| `lease-window`, `supply`, `namespace-installation`, `namespace-layout`, `namespace-retirement` | Rights/window intersection; acquisition/rollback/release/device retirement; retained grants and publication compensation; shared/instance placement authority; revoke/unbind/unpart acknowledgments before storage release. Owners: Supply, Installation, Operator/Global Layout. |
| `operator-api`, `operator-connection`, `operator-handoff`, `operator-watch` | Bounded typed rights; endpoint ownership/lifetime; holder/source restriction and retained handoff; notification order, cursors and retirement. Owner: Operator. |
| `system-shape` | Retire old universal generic arity, ECS/injected-hook/path/layout assertions. Port only semantic behavior: retirement ordering, pause/resume, dependency direction, and owner privacy. No current evidence is implied by removing shape checks. |

This is a finite assertion-port queue, not a claim that each listed risk
already has a one-to-one current test. In particular the old assertion that
provider settlement precedes Stop requires real native acknowledgment evidence;
the Host mock/script or a matching test name alone is insufficient.

## Completion status

M0 is a fixed baseline, not a completion or all-green acceptance report. Its
selected inputs, original matrix and recorded CI result remain preserved above.

## Current migration status

The tables above are the fixed M0 snapshot at System
`332d9d2e15f6bc4e1e8d94e4db2591b1a53ae613`. They remain unchanged as historical
baseline evidence. The current status below was checked against System
`34b8aa9349795a7b489f68222a1812516fb2bada`; it does not rewrite M0's CI result,
protected matrix or acceptance rules.

### M1: Host owner migration and history

The original 114-function Host set is represented in the current suites, which
now contain 130 functions: 92 Server tests (`app` 7, `control` 19,
`control-retirement` 5, `identity` 19, `loader` 3, `operator` 21,
`operator-watch` 4, `runtime` 6, `supply` 8) and 38 API/Client/Support tests.
The additional 16 assertions came from follow-on migration and regression
coverage. The candidate Rust Runner has 11 unit tests, and the App pack checker
has 4; neither count is included in the 130 Host functions.

All production `#[cfg(test)]` hooks have been removed from `server/src/`.
Private owner behavior is compiled through test-local facades. Recent Control
work extracted private publication and shutdown policy helpers while keeping
the effect order; the predicate suite grew from 17 to 19 tests. This is a
production source refactor, not a claim of no production code changes.

Two historical packages have been removed after migration: `dynamic-units`,
whose stable declaration assertions now live in App Host tests, and
`capability-import`, whose stable Watch owner transitions now compile the real
Operator Watch source with scripted Host Kernel/transport effects. The rest of
the historical safety queue remains open where stable invariants lack a
current owner assertion or require native evidence. See the assertion-level
[historical inventory](test-inventory.md), [service map](service-test-migration.md),
and [Control map](control-test-migration.md). Host fakes do not establish
native Kernel behavior or product acceptance.

### M2: target migration and candidate runner

The target definitions are under `server/tests/target/`, and the Rust host
runner is under `server/tests/runner/`. A candidate result is not final parity
sign-off. On System `59b1a65`, the retained Native workflow run
[43812239225](https://github.com/shuyra-arche/system/actions/runs/43812239225)
passed 22/23 jobs; `Handoff / supply / Kernel debug / System debug / Harts 1`
failed. Candidate Rust Runner run
[43812239288](https://github.com/shuyra-arche/system/actions/runs/43812239288)
passed 23/23 jobs on that same System revision. A same-old-archive replay using
the generic Rust process engine and current System checker passed, showing the
failure is timing-sensitive and not simply a Runner check omission. The full
scenario-by-scenario equivalence review and final acceptance are still open.
This later evidence is separate from M0's 22/23 result at run 43802055998 and
its Handoff timeout; neither should be reported as an all-green old gate.

The same-old-archive replay using the generic Rust process engine and current
System checker passed. This supports the timing-sensitive diagnosis; it is not
the final scenario-by-scenario equivalence review. Raw execution logs remain
invocation/CI artifacts rather than permanent documentation inputs.

### M3: CI transition

The original 23-job Native CI and its Python runners remain the repository's
gates. The candidate Runner has not replaced that workflow, and M3 CI transition
is not complete.

The runner's process-settlement helper comes from independent Kernel PR #219,
selected through System's Host-only `kernel-verification` pin
`ad360f86c9472d79d6c087edb8d03aa1f0906161`. Product/native Kernel remains
`789ffd19b5359f3de172fb9aad8a61f6a7113a64`; this helper pin does not change the
provider selected by native builds.

### M4: clean-checkout and product acceptance

M4 clean-checkout acceptance is not complete. Full System/Toolchain, hardware,
public S1/S2, and cross-provider product acceptance remain separate and open.
No behavior, deadline, retry rule, provider pin, profile/Hart selection or
acceptance criterion is relaxed by the candidate runner or this status update.

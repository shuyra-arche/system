# Independent Team handoff

This native no_std image runs as an ordinary User Unit under the complete
production System Server. It is not a fixture replacement for System, a
Toolchain std Unit or a bootable Hub implementation.

The controller receives a restricted Instance endpoint through real System
Supply. It creates Driver instances with System IDs and queryable operation
receipts. An independent observer obtains the same endpoint but cannot inspect
or stop the controller's instances. Drivers receive an actual narrowed Sign
lease from the controller's native provider and obtain their own Task-scoped
Operator entrances through OwnRoot. Ordinary entrances cannot register or
advance UnitRoots. A Driver publishes a Clone RPC service itself.

The controller resolves a stable root and calls the Driver directly. Replacing
the association keeps the old consumer Pend usable until explicit retirement.
Lease withdrawal is tested while the Driver is still running; Busy is not
accepted as revocation evidence. Other checks include exact replay/conflict,
ignored first reply with operation query, pending cancellation, stale issuer,
completed-receipt cancellation, provider cleanup and an early owner exit before
native Task construction. The checker requires typed Task exit/retired order,
independent Team identities and Kernel's admitted initial-System exit.

Release Kernel runs give each User role a 60-second scenario budget and each
VM a 90-second limit. Debug Kernel runs use the recorded 120/180-second bounds
described below. Main recovery, authorization/replay,
lost-reply/pending-cancel and early owner death run as separate archives/VMs,
so contract and fault checks do not
consume the main recovery scenario's budget. The initial fixture incorrectly
killed a caller after five seconds of an asynchronous request/operation, while debug Kernel
was still preparing another ordinary ELF. Waiting now uses the SDK's native
reply watch and queries the original operation at a 500 ms cadence under the
scenario budget. The idle Driver waits on native ingress readiness. The
observer discovers at a two-second cadence; repeated test obligations are
exercised once rather than issuing redundant RPCs for both Drivers.
The authorization observer acknowledges through the real Driver RPC; its
controller waits for that acknowledgment before closing discovery.
During one Pending-operation wait, only the authorization Controller also derives
32 Object-scoped Clone PEEK grants from its real local source to the System Task
that owns its Instance endpoint, then revokes every returned grant. Every revoke
must complete; exactly one full batch must run and the local source is returned
before passing. All waits retain the 500 ms cadence. The serial records observed
Busy responses, which depend on scheduling and are not required for a passing
single-Hart run. This is a bounded concurrency probe, not deterministic proof of
a Busy response. The separate pre-fix native stress diagnostic for
[System #21](https://github.com/shuyra-arche/system/issues/21) captured discovery
grant Busy followed by installation failure.
The Driver retains the exact RPC grant request and retries a native Busy within
its role budget before publishing that grant to Operator. A contended admission
does not prove successful publication or revocation; all subsequent grant and
withdrawal assertions remain mandatory.
Production memory/retirement deadlines are unchanged. System grants each
declared startup Unit 60 seconds, advancing the deadline only when the previous
startup Unit reaches native readiness or retirement. A multi-Unit catalog does
not share one global 60-second interval. Busy, polling and partial construction
do not renew a stalled Unit's deadline. Native cleanup steps still have their
own bounded retries.

Use the candidate Rust `handoff` subcommand and the instruction-qualified
platform described in the [application guide](../app/README.md):

```sh
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- handoff --kernel PATH --output NEW --profile release --kernel-profile release --harts 1 2 4
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- handoff --kernel PATH --output NEW --profile debug --kernel-profile release --harts 1 2 4
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- handoff --kernel PATH --output NEW --profile release --kernel-profile debug --harts 1 2 4
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- handoff --kernel PATH --output NEW --profile debug --kernel-profile debug --harts 1 2 4
```

The second invocation selects a debug System with a release Kernel; the third
pairs a release System with a debug Kernel. The fourth pairs both debug
profiles. Use the matching qualified Kernel ELF for each
`--kernel-profile`. The Rust runner and old/new parity remain under migration;
these examples do not claim a runner pass. Keep the existing `native.yml`
checks until candidate coverage, evidence and failure behavior are equivalent.

The default handoff matrix also runs `owner`, `owner-cancel` and `owner-death`
as independent VMs. Trusted Selected policy admits CreateFor; an ordinary
Create and absent owner are denied. Requester replay/conflict and operation
queries remain requester-local, while only the selected owner can enumerate
and manage the attempt. The lifecycle case starts the Driver after the
requester actually exits, checks UnitRoot owner identity and physical
retirement. Separate cases prove requester cancellation waits for cleanup and
positive selected-owner exit closes its attempt. Owner death may race prepared
completion: its historical CreateFor receipt remains Completed if already
committed, otherwise the retired failed attempt reports Interrupted. The typed
checker still requires every native exit/retirement and the real System exit.

Use `--scenarios owner owner-cancel owner-death` for focused checks; omitting
it retains every scenario. For example:

```sh
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- handoff --kernel PATH --output NEW --profile release --kernel-profile release --harts 1 2 4 --scenarios owner owner-cancel owner-death
```

The original selected-owner campaign adds 36 cases to the first 48. Release
Kernel runs retain 60-second role and 90-second VM bounds.
With `--kernel-profile debug`, the recorded bounds are 120/180 seconds; see the
[production application guide](../app/README.md) for the measured migration cost.

The User image stays release with LTO and one codegen unit in both cases.
System and Kernel retain their separately selected profiles and production
flags. Pair each with recorded debug and release Kernel images; a host test or RV64 build is separate evidence. Each
invocation keeps its archive, ELFs, locks, hashes, serial logs and JSON results
under the caller-selected `--output NEW` directory. Failed checks remain failed
receipts. Only after every behavioral assertion passes, a missing admitted
root-exit line accompanied by a positive Kernel `console-lost` count is marked
inconclusive (checker status 2). The runner preserves that log/JSON with
`passed: false` and qualifies one fresh VM. That VM must satisfy every assertion,
including the actual root-exit line. No timeout, fault or other assertion is
retried or accepted; a second inconclusive result fails. The
[contract matrix](../../../docs/instance.md) records supported operations and the remaining S1/S2 acceptance scope.

The `supply` scenario uses the public System Supply registration operations,
with no private native Source in that path. Its provider registers before Ready;
System installs the declared Sign into independent Drivers. It checks explicit
and ancestor withdrawal, cancellation-before-register, original provider rights,
ordinary self Acquire and fresh registration selection from the same live Driver.
The campaign retains those eight scenarios (96 profile/Hart cases). See
[the contract](../../../docs/supply.md) for ownership and proof limits.

The `install` scenario creates a Driver without declared device bindings and
waits for completed Prepared before creating/registering Sign sources. Public
Install operations then deliver three independent slots to the observed actual
Task before Start. Replay, lost reply, foreign caller/target/source/slot/rights,
pre-admission and accepted cancellation, sibling survival, live source withdrawal
and target retirement are checked with real Kernel effects. `install-loss` is a
separate VM: a required source disappears while Prepared; the target must retire
without Ready, settle Released and reject late Start. All original eight scenarios
remain in the default eleven-scenario campaign (132 profile/Hart cases), with the
same role/VM/startup bounds and artifact hashing.

The runner builds the seven lifecycle/owner scenarios with the default fixture,
external Supply with `--features provider`, and post-Prepared operations with
`--features install` (which enables provider). Each uses a separate target
directory. Every image stays release/LTO; the production Server is identical.
This keeps new Install RPC monomorphizations out of every legacy Driver image.
The initial combined image added 48,600 loaded bytes (96,432 ELF file bytes);
a double-debug one-Hart legacy recovery VM completed its assertions and admitted
System exit but missed the unchanged 180-second whole-VM bound. That failed
receipt remains in PR #27. No timeout retry or budget increase accepts it.
The runner records/copies every selected User ELF with its feature set and hash;
each VM receipt identifies the actual selected image and archive hash.

The optional `supply-contend` VM uses the `provider` fixture feature and adds a
separate User Team repeatedly deriving/revoking Object Clone grants to the real
System recipient while public Sign source replacement and Driver construction
run. Every successful grant and its local source must actually retire. CI runs
this bounded probe on release Kernel/System at 1/2/4 Harts (three additional VMs;
the default eleven-scenario/four-profile campaign is 132). Busy counts depend
on scheduling; zero is valid. The pre-fix two-Hart diagnostic reproduced static
CPU source registration Pending being treated as permanent construction failure.
Readiness Sign delivery exposed the same missing CPU retry boundary. Production
now retries only pre-publication Object Sign/Clone Busy in the existing five-second
budget while retaining the original owner and request. Other kinds retain their
original effect/retirement handling. The consumer fixture also retains its same
LeaseId/request/deadline across a pre-commit Acquire Pending. Failed VMs remain
failed receipts; no failed assertion or timeout is retried as acceptance.

A later exact-head CI still timed out in the original Supply VM on double-debug
two Harts: all provider assertions and typed Complete appeared, but the admitted
root exit did not occur within 180 seconds (artifact 14192476785). The provider
image now excludes unrelated legacy lifecycle/owner entry paths as well; its
PT_LOAD file bytes are 319,496 locally versus the prior shared fixture's
405,832 bytes. Lifecycle, provider and install feature selections, exact ELFs and hashes
remain recorded per VM. Every role/VM/startup limit and behavioral/exit assertion
is unchanged. This failed receipt is preserved rather than retried as acceptance.

CI separates debug-Kernel lifecycle and supply scenario groups so the full
campaign fits its unchanged 20-minute job bound. Each Hart/profile still runs
every original and added scenario; only the job boundary and artifact suite name
change. Native role/VM deadlines are not extended. The preceding combined jobs
on double-debug one/two Harts were cancelled at 20 minutes during install-loss
(CI 43793245104); their partial receipts remain unaccepted evidence.

The independent `supply-auth` VM now carries the existing native provider
name/rights, replay/conflict, consumer-ID isolation and cancellation-tombstone
assertions. The `supply` VM still builds both Driver generations and proves
same-live-Driver source replacement, discovery CAS, explicit/ancestor withdrawal,
original provider rights and complete retirement. The old combined two-generation
role exhausted its unchanged 120-second debug deadline in CI 43795947454; its
failed log is retained. Moving the admission obligations to their own VM keeps
all assertions while giving each bounded scenario its own existing role budget.
The default is eleven scenarios/132 profile-Hart cases plus three CPU pressure VMs.

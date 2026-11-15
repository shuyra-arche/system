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

Each User role has one 60-second scenario budget; the outer runner still has
its original 90-second limit per VM. Main recovery, authorization/replay,
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
Production memory/retirement deadlines are unchanged. Native cleanup steps
still have their own bounded retries.

Use the instruction-qualified platform and runner documented in
`../system/README.md`:

```sh
python3 tests/guest/system/run.py /path/to/qualified/kernel --handoff --harts 1 2 4
python3 tests/guest/system/run.py /path/to/qualified/kernel --handoff --profile debug --harts 1 2 4
```

The User image stays release with LTO and one codegen unit in both cases.
System and Kernel retain their separately selected profiles and production
flags. Pair each with recorded debug and release Kernel images; a host test or RV64 build is separate evidence. Each
invocation keeps its archive, ELFs, locks, hashes, serial logs and JSON results
under `tests/guest/system/target/handoff/<profile>/`. Failed checks remain failed
receipts. Only after every behavioral assertion passes, a missing admitted
root-exit line accompanied by a positive Kernel `console-lost` count is marked
inconclusive (checker status 2). The runner preserves that log/JSON with
`passed: false` and qualifies one fresh VM. That VM must satisfy every assertion,
including the actual root-exit line. No timeout, fault or other assertion is
retried or accepted; a second inconclusive result fails. The
[contract matrix](../../../docs/instance.md) records supported operations and the remaining S1/S2 acceptance scope.

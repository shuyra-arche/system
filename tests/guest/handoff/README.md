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

Use the instruction-qualified platform and runner documented in
`../system/README.md`:

```sh
python3 tests/guest/system/run.py /path/to/qualified/kernel --handoff --harts 1 2 4
python3 tests/guest/system/run.py /path/to/qualified/kernel --handoff --profile debug --harts 1 2 4
```

The User image stays release in both cases. Pair each with recorded debug and
release Kernel images; a host test or RV64 build is separate evidence. Each
invocation keeps its archive, ELFs, locks, hashes, serial logs and JSON results
under `tests/guest/system/target/handoff/<profile>/`. Failed checks remain failed
receipts. The [contract matrix](../../../docs/instance.md) records supported
operations and the remaining S1/S2 acceptance scope.

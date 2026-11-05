# System

System owns its `system` boot executable and `server/unit.toml`. The host
Vanllia CLI selects and packages necessary Unit records and images into initrd;
System validates those records, owns the decoded runtime policy and instantiates
Units according to startup/requested creation. Deployment stays in host memory.

`server/` owns execution, identity, namespace, readiness, supervision and resource
retirement. `support/` exposes the validated borrowed initrd catalog and boot
facts. Operator and Identity bootstrap remain internal System threads. `api/`
and `client/` keep their existing public protocol responsibilities.

Runtime declarations use shared owned arrays and strings borrowed from genuine
immutable initrd storage. System forwards typed lifecycle frames with native task
exits and acknowledged reclamation. Integration verdicts do not depend on kernel
event logging or diagnostic strings.

The new bundle reader is selected as the `initrd` dependency. Existing `env` ABI
dependencies keep their pinned type identity. The same-repository support crate
is a path dependency; externally consumed APIs retain their public Git pins.

```sh
cargo build -p system-server --bin system --target riscv64gc-unknown-none-elf --locked
cargo test --manifest-path tests/dynamic-units/Cargo.toml --target x86_64-unknown-linux-gnu --locked
```

Validation used installed nightly-2026-10-09; the historical repository toolchain
pin is unchanged. See [validation/DYNAMIC-UNITS.md](validation/DYNAMIC-UNITS.md).
Old scenario workspaces and test-support fixtures are not migrated by this
change, and their historical results are not evidence for the new reader.
No guest or SMP acceptance was run for this migration.

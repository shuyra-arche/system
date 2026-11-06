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
paths forward to the canonical Kernel API, and Execution uses Kernel Client.
The same-repository Client and Support crates are workspace dependencies;
externally consumed protocols retain their public Git pins. The default branch's
Unit policy, five runtime resource declarations and boot behavior are preserved.

```sh
cargo build -p system-server --bin system --target riscv64gc-unknown-none-elf --locked
cargo test --manifest-path tests/dynamic-units/Cargo.toml --target x86_64-unknown-linux-gnu --locked
```

Validation used installed nightly-2026-10-09; the historical repository toolchain
pin is unchanged. See [SDK dependency validation](SDK-VALIDATION.md) for the
current default-branch SDK selection and standalone executable checks, and
[dynamic Unit validation](validation/DYNAMIC-UNITS.md) for the earlier reader.
Old scenario workspaces and test-support fixtures are not migrated by this
dependency update. The historical `5a792ff` library-only SDK result is distinct
from the current default's executable; historical fixture counts are not new
acceptance evidence. No new guest or SMP acceptance is claimed by this update.

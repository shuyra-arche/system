# Native System Host runner

This standalone Cargo package drives the native System guest scenarios with
bounded Cargo and QEMU processes. Kernel process and Capsule helpers are pinned
to kernel-verification revision
789ffd19b5359f3de172fb9aad8a61f6a7113a64. Application and handoff verdicts
remain owned by server/tests/target/app/pack; this runner invokes its existing
archive builder and typed event checker.

Run from the repository root. --output must name a new directory; every
invocation retains build logs, QEMU stdout/stderr, input hashes and per-run
receipts there.

```sh
cargo run --locked --manifest-path server/tests/runner/Cargo.toml \
  --target x86_64-unknown-linux-gnu -- runtime \
  --kernel /path/to/qualified-kernel --output /tmp/system-runtime-run

cargo run --locked --manifest-path server/tests/runner/Cargo.toml \
  --target x86_64-unknown-linux-gnu -- entry \
  --kernel /path/to/qualified-kernel --output /tmp/system-entry-run \
  --harts 1 4 [--abort | --stall]

cargo run --locked --manifest-path server/tests/runner/Cargo.toml \
  --target x86_64-unknown-linux-gnu -- loader \
  --kernel /path/to/qualified-kernel --output /tmp/system-loader-run \
  --profile debug --harts 1 2 4 [--toolchain-receipt /path/to/receipt.json]

cargo run --locked --manifest-path server/tests/runner/Cargo.toml \
  --target x86_64-unknown-linux-gnu -- application \
  --kernel /path/to/qualified-kernel --output /tmp/system-app-run \
  --profile release --kernel-profile release --harts 1

cargo run --locked --manifest-path server/tests/runner/Cargo.toml \
  --target x86_64-unknown-linux-gnu -- handoff \
  --kernel /path/to/qualified-kernel --output /tmp/system-handoff-run \
  --profile release --kernel-profile release --harts 1 2 4 \
  [--scenarios handoff auth cancel death owner owner-cancel owner-death supply supply-auth install install-loss]
```

Set QEMU_RISCV64 and QEMU_OPENSBI to select the QEMU binary and firmware;
the selected QEMU version and firmware hash are recorded. Runtime and Entry
retain their release-only fixtures. Loader's default hart set is 1/2/4;
Runtime and Entry default to 1/4; Application and Handoff default to 1.
Explicit harts must remain in each scenario's qualified set.

Runtime and Entry QEMU calls are bounded to 120 and 90 seconds. Loader uses
120 seconds. Application uses 90 seconds; Handoff uses 90 seconds with a
release Kernel and 180 seconds with a debug Kernel, retaining the matching
60/120-second role budget. Only the existing Handoff pack checker result 2 for
kernel console loss permits one fresh VM. Failed builds, nonzero exits,
timeouts, missing or duplicated markers, failure text, missing output streams,
and failed checker verdicts remain failures.

On Linux, child processes receive PR_SET_PDEATHSIG; process-group termination
also bounds timed-out commands. A hard kill of this runner kills each direct
child, but Cargo may already have spawned compiler descendants; they may outlive
that direct child. Do not treat cancellation as complete process-tree cleanup.

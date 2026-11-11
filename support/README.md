# System support

This unconditional `no_std` package reads immutable Unit bundles, typed Kernel
ABI 9 boot resources and bounded DTB slices. It depends only on Kernel API/Client,
Unit's pure declarations (`default-features = false`), FDT and Erra. There is no
Env, Execution, Resource or driver API dependency in its resolved target tree.

`Accounts::parse` borrows supplied bytes. `unsafe Accounts::from_words` accepts
exactly the Kernel's four startup words; the entry caller guarantees valid,
immutable mappings and their lifetime. ControlId and PendId remain separate.
Interrupt and memory-observation entries preserve the separately authorized
Share/Sign pair. No untyped `token` accessor or ambient startup discovery exists.
`Catalog` validates VNI1 framing through Unit and the opaque capsule through the
current Kernel parser. Unit records and ELF payloads are validated by their
respective System consumers before use.

`Machine::of` accepts a bounded DTB slice rather than expanding a raw FDT pointer.
The device inventory keeps its existing 62-entry capacity without importing Hub.
Native face creation requires an explicit creation license and returns queue
ownership to its caller. Workload calibration uses measured monotonic nanoseconds;
the removed timer-tick estimate is not an ABI 9 clock source.

```sh
cargo test --locked -p system-support --tests --target x86_64-unknown-linux-gnu
cargo clippy --locked -p system-support --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
cargo clippy --locked -p system-support --lib --target riscv64gc-unknown-none-elf -- -D warnings
cargo tree --locked -p system-support --target riscv64gc-unknown-none-elf
```

The host tests cover borrowed mappings, typed authority domains, paired resources,
malformed/old/duplicate ledger entries, truncation and current capsule validation.
Both the declared `nightly-2026-09-04` and the migration's newer nightly were checked.
These tests do not establish the full System boot path or safe device withdrawal.

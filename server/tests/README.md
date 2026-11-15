# Server Host contracts

Run from the repository root:

```sh
cargo test --locked --manifest-path server/tests/Cargo.toml --target x86_64-unknown-linux-gnu
cargo clippy --locked --manifest-path server/tests/Cargo.toml --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
```

The explicit `supervision` target compiles actual declaration, ELF admission,
Program, Operation Journal and construction Ledger sources. It preserves policy
checks for native rights/window conversion (including Nest's reserved-bit gap),
invalid identities/limits, startup selection, missing/cyclic dependencies,
borrowed record strings and explicit VNU1 rejection. Program intent and retained
receipts remain separate from physical Instance evidence.

The `runtime` target supplies a scripted memory transport to the actual memory
owner. It checks retained requests, version ordering and continuation rules;
it does not execute Kernel traps. Test facades live here rather than in
production source. RV64 builds, native QEMU runs and hardware qualification are
separate evidence; see the root README and [migration inventory](../../docs/test-inventory.md).

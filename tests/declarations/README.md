# Native Unit policy checks

This independent host suite uses the actual production declaration interpreter
and pure Unit schema-2 provider, without the legacy Server graph.

```sh
cargo test --locked --manifest-path tests/declarations/Cargo.toml --target x86_64-unknown-linux-gnu
cargo clippy --locked --manifest-path tests/declarations/Cargo.toml --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
cargo check --locked --manifest-path tests/declarations/Cargo.toml --target riscv64gc-unknown-none-elf
```

The five tests protect typed rights/window conversion (including Nest's reserved
bit gap), invalid identities/limits, startup/requested selection, missing/cyclic
policy dependencies, borrowed record strings and explicit VNU1 rejection. They
are policy checks, not proof of runtime authorization or full-System operation.

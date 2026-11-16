# Server Host contracts

Run every Server Host contract from the repository root:

```sh
cargo test --locked --manifest-path server/tests/host/Cargo.toml --target x86_64-unknown-linux-gnu
cargo clippy --locked --manifest-path server/tests/host/Cargo.toml --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
```

The manifest exposes focused suites by semantic owner:

| Target | Protected behavior |
| --- | --- |
| `app` | 7 schema-2 declaration and startup selection tests |
| `control` | 19 caller-scoped construction/install journal, publication, shutdown dependency and Program lifecycle tests |
| `identity` | 19 Identity book, monotonic revision publication and decoded service dispatch tests |
| `loader` | 3 ELF admission, segment and entry validation tests |
| `operator` | 21 authority, namespace, roots, admission policy and runtime state tests |
| `runtime` | 6 scripted memory delivery, retained version, mutation identity and continuation tests |
| `supply` | 8 external registration, withdrawal and installation selection tests |
| `control-retirement` | 5 settlement, cleanup, release and cross-Instance retirement tests with scripted provider effects |
| `operator-watch` | 4 subscription authority, replacement, prefix and cleanup tests with scripted Kernel/transport effects |

The nine targets contain 92 tests and can be selected independently with
`cargo test --manifest-path server/tests/host/Cargo.toml --test <target>`.
The additional `control-retirement` target checks the real Instance retirement
gate with scripted provider responses, cleanup failures and release retries.
Its separate compilation boundary supplies a transport facade; it does not
establish native provider settlement.

The Host manifest lives under `host/` so Cargo's default output and `cargo clean`
cannot collide with the source packages under `target/`.

These suites import production source through test-local facades where private
source composition is required. No `#[cfg(test)]` hooks remain in `server/src/`;
the Host targets compile production owners through separate test entry points.
Production packages retain `no_std`; host tests
do not execute Kernel traps or establish RV64 or QEMU behavior. See the
[migration status](../../docs/native-test-baseline.md) for the fixed M0 snapshot
and current progress, and the [historical assertion maps](../../docs/test-inventory.md)
for the remaining owner-level work.

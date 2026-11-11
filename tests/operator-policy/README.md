# Operator authority and admission

This host fixture compiles production Operator authority state and admission
policy against the current workspace protocol and native Kernel API. It checks
root admission, per-caller and per-entrance replay, cancellation watermarks,
independent source constraints, scope relocation and ordered retirement after
storage compaction. Native Kernel resource effects are checked separately by
`tests/guest/client/run.py` on one and four Harts.

```sh
cargo +nightly test --locked --manifest-path tests/operator-policy/Cargo.toml --target x86_64-unknown-linux-gnu
cargo +nightly clippy --locked --manifest-path tests/operator-policy/Cargo.toml --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
```

The state owner supplies transport-authenticated caller and native ingress
coordinates. Relative anchor paths come from its own namespace tree. State
transitions require successful native attach, grant delivery, detach and Return
receipts. A failed Return retains the entrance at Detached; the owner must retry
it before closing the record. Revocation immediately denies descendants even
while resource retirement is pending. This fixture does not establish a migrated
Operator service loop or an assembled System.

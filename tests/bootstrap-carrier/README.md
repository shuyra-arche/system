# Bootstrap carrier

From the repository root:

```sh
cargo test --manifest-path tests/bootstrap-carrier/Cargo.toml --locked --target x86_64-unknown-linux-gnu
```

The suite compiles the production Assembly carrier and client decoder. ABI and
wire frames are real; effects are host fixtures. It checks Pending cursor
ordering, failed carrier boundaries, retry root identity, release failure,
publication timeout, independent children, permissions and untrusted frames.

Control completion is covered by `tests/control-instance`. This is a legacy
fixture: current `test-support` compatibility is unresolved as described in the
root README. Kernel authority, actual boot and reclamation need guest tests.

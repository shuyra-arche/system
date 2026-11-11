# Identity service state

This fixture compiles the production Identity book and request dispatcher with
the workspace protocol v2 API. It has no Env, Wire or old Kernel dependencies.
The book's historical behavior tests now live here, outside production source.

```sh
cargo +nightly test --locked --manifest-path tests/identity-state/Cargo.toml --target x86_64-unknown-linux-gnu
cargo +nightly clippy --locked --manifest-path tests/identity-state/Cargo.toml --all-targets --target x86_64-unknown-linux-gnu -- -D warnings
```

The state tests preserve inheritance, attenuation, membership eligibility,
revocation through origin/current snapshots, canonical paging and cursor
revisions, quota failures without partial nodes and caller-scoped Found retry
identity. Dispatcher tests decode actual v2 request envelopes before invoking
the production handler. All 24 ordinary operations reject the wrong endpoint
grant before reading or mutating state. Installation tests cover idempotence,
cancellation tombstones, closed transaction watermarks, retirement and an old
installation's inability to remove a replacement binding.

`Same` retains Yes, No and Unbound as independent wire results. A claimed task
inside a request never substitutes for its authenticated caller context. Named
references retain explicit root versus entry-zero identity and installer-only
mutation.

`tests/guest/client` also exercises the production Identity book and dispatcher
through real Twine queues and service ingress, including installation,
inspection, cancellation, retry, retirement and endpoint grant rejection. It
observes the service Task's Kernel exit result and reclamation before deleting
the Task. The full System actor loops, notifications and boot assembly still
need migration; this fixture does not establish their integration.

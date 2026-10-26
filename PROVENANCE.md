# Provenance

## Source

- Repository: <https://github.com/anreonyr/sqware.git>
- Commit: `e6302af2156a6fe3766d3ccc22f080a481948bf3`
- Branch at extraction: `codex/shuyra-system-api-extraction`
- Extracted paths: `programs/src/system/api`, `programs/tests/identity-api`,
  `programs/tests/loader-api`

This is a snapshot of public source commit `e6302af`. It is not a rewrite of
the source history. The published repository commit that contains this snapshot
is decided by the root publication stage and is not asserted here.

## File mapping

| Source at `e6302af` | Here |
| --- | --- |
| `programs/src/system/api/src/**/*.rs` (41 files) | `api/src/**/*.rs` |
| `programs/src/system/api/Cargo.toml` | `api/Cargo.toml` (standalone rewrite) |
| `programs/tests/identity-api/lib.rs` | `tests/identity-api/lib.rs` |
| `programs/tests/identity-api/Cargo.toml` | `tests/identity-api/Cargo.toml` (path edit only) |
| `programs/tests/identity-api/.gitignore` | `tests/identity-api/.gitignore` (lock line removed) |
| `programs/tests/loader-api/lib.rs` | `tests/loader-api/lib.rs` |
| `programs/tests/loader-api/generation.rs` | `tests/loader-api/generation.rs` |
| `programs/tests/loader-api/Cargo.toml` | `tests/loader-api/Cargo.toml` (path edit only) |
| `programs/tests/loader-api/.gitignore` | `tests/loader-api/.gitignore` (lock line removed) |
| source-root `rust-toolchain.toml` | `rust-toolchain.toml` |
| source-root `Cargo.lock` | `Cargo.lock` (pruned copy) |
| source worktree test locks (untracked) | `tests/*/Cargo.lock` |
| new | `api/tests/shape.rs` |

All 41 API Rust files are byte-for-byte copies. ABI, layout, authorization,
lifecycle, protocol IDs, marks, codecs, and the 130-byte `SupplyFrame`
descriptor are untouched. The three original test Rust files
(`identity-api/lib.rs`, `loader-api/lib.rs`, `loader-api/generation.rs`) are
also byte-for-byte copies; they still contain 9, 5, and 1 tests respectively.

The API manifest is rewritten only to stand alone: inherited workspace fields
become explicit `version = "0.1.0"` and `edition = "2024"`, and the original
`[lib] test = false`, `bench = false`, `doctest = false` settings are kept.
The normal dependencies remain exactly `env`, `wire`, and `mold` at the
published revisions:

- `env` `d3000253cd5d13d466e91fafd4dbb27a5ec5aaae`
- `wire` `0ff77dc4573fa2620d2fe995a18c01949d9c7e57`
- `mold` `f52501f4c91b686eded8b4bcd743b4e96084b7f9`

Each test manifest keeps its original package name, explicit version and
edition, `publish = false`, independent `[workspace]`, and `[lib] path =
"lib.rs"`. Its only edit is `system-api`'s path, now `../../api`. The test
`.gitignore` files originally ignored `/target/` and `/Cargo.lock`; only the
lock line was removed so the owned locks can be tracked.

## Shape assertions

`api/tests/shape.rs` is new. It adds three tests while preserving the four
original assertions:

| Test | Assertions | Source |
| --- | --- | --- |
| `provider_api_keeps_one_way_dependencies` | API normal dependencies are all in `env`/`wire`/`mold`, using the original `dependencies` helper | `programs/tests/system-shape/lib.rs:562-564` |
| `api_has_no_control_publication_module` | `api/src/control/publication.rs` does not exist | `programs/tests/system-shape/lib.rs:775` |
| `sdk_separates_read_discovery_from_entry_capabilities` | `api/src/operator/mod.rs` contains neither `pub enum Grant` nor `mod grant` | `programs/tests/operator-api/lib.rs:481-483` |

The first test reads this repository's `api/Cargo.toml`. The second reads this
repository's `api/src/control/publication.rs` path, so it cannot pass merely
because the source tree removed the file. The third reads this repository's
`api/src/operator/mod.rs`. No behavior assertion was added, removed, or
weakened.

At source commit `e6302af`, the old four assertions remain in
`programs/tests/system-shape/lib.rs` and `programs/tests/operator-api/lib.rs`.
The consumer stage after publication owns removing those old halves. The
source test `provider_api_and_public_clients_keep_one_way_dependencies` keeps
its client, terminal, router, and hub checks; only the API dependency half maps
here. The source test
`account_has_its_own_task_and_api_and_uses_authorized_control_contract` keeps
its other checks; only the publication-path assertion maps here. The source
test `sdk_separates_read_discovery_from_entry_capabilities` keeps its
client/Capability/Rein/granted_berth checks; only the two API operator
assertions map here. The source registry APIS/LOADER combination checks were
not moved.

## Lock files

The source commit tracks its root `Cargo.lock`, but does not track the two test
locks: `programs/tests/*/Cargo.lock` are generated in the source worktree and
ignored by the tracked test `.gitignore` files. This repository tracks all
three locks.

- `Cargo.lock` was copied from the source-root tracked lock and pruned by an
  actual offline `cargo metadata` run to this workspace's graph. It keeps the
  registry versions already accepted in that lock, including
  `bitflags 2.13.1` and `unicode-ident 1.0.24`.
- `tests/identity-api/Cargo.lock` and `tests/loader-api/Cargo.lock` were copied
  from the source worktree's generated locks and resolved by their own Cargo
  workspaces. They keep the accepted registry versions from those locks,
  including `bitflags 2.13.2`, `proc-macro2 1.0.107`, `quote 1.0.47`,
  `syn 2.0.119`, and `unicode-ident 1.0.24`.

The three locks are generated extraction artifacts, not upstream API files. They
are included here so the publication commit tracks them and builds can use
`--locked`. No registry version was upgraded for this extraction.

## Attribution

The source commit author and committer is
`anreonyr <anreonyr@gmail.com>`. Author identities observed in the real source
Git history include:

```text
anreonyr <anreonyr@gmail.com>
anreonyr <88045926+anreonyr@users.noreply.github.com>
```

The original repository history remains at the source URL above. This snapshot
does not synthesize or rewrite that history.

## Licensing

The source project declares no license. At commit `e6302af`, no tracked
`LICENSE`, `COPYING`, or `NOTICE` file matched the project. No license text or
license metadata is added by this extraction; do not assume a license that the
source does not grant.

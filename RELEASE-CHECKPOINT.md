# System server provider checkpoint

Source: `anreonyr/sqware` at `a29707fb83191647c63cc9ccfef1e8721bbe8dad`.
Provider extends `9e5d63ca536a176583cf1b0054b4ecf7ddd88257`; original API/history
and license status remain unchanged. `MIGRATION-MAP.json` records file hashes;
`TEST-OWNERSHIP.md` records mixed assertion ownership.

`system-server::run` consumes one Boot, explicit UnitFile declarations and one
already-selected Deployment. Source owns actual composition, ELF entry and
selection. Boot/callback evaluation and scene validation retain their original
order. Shared boot parsing, machine parsing, logging, timing and tick calibration
are single-sourced in neutral `system-support`. Source supplies the PLIC class.

The optional `test-support` feature owns the original private runtime fixtures,
bench and fault/recovery scenarios. Source retains its account/terminal/login,
UART and composition policy through typed inputs/callbacks. No Registry, Control,
Jobs, Roster, Tree, Table, Loader or Plan is exposed to Source. AccountRig owns the
original account-local hook plan and locals; it does not duplicate the four
generic Fixture plans or create a new production supervisor. Existing capability
shipping/rights and all original assertions, waits and reclaim gates are kept.

## Actual validation in this cloud instance

- Server host library: 23 tests, raw 0; explicit `--lib` required.
- Server with `test-support`: host 23 tests, raw 0, locked/offline.
- Provider RISC-V checks: debug/release, with and without test support, raw 0;
  locked/offline repeats recorded. Optional device API lock additions were
  Cargo-generated, not handwritten.
- API-owned suites: shape 3 + identity 9 + loader 6, raw 0, locked/offline.
- 24 migrated owned host suites: 280 tests, final raw 0, locked/offline.
- Split provider suites: system-shape 19 + control-rpc 9 + watch 3, raw 0,
  locked/offline. Pure runtime assertions retain the >100 signature count and
  restricted visibility/schedule ordering checks.
- Source halves: shape 7, Hub 5 and account 1 pass, locked/offline raw 0. The first account-half compile exposed a
  missing Message trait import after splitting, not a weakened protocol test.
- Selected server graph uses one old Git system-api source matching unit. Normal
  server has no Source dependency. Device test APIs use public exact revisions.

Raw stdout/stderr, exit codes, session/model evidence and review records are
retained in `/workspace/evidence/system` and `/workspace/evidence/preflight`.
The independent DeepSeek reviewer accepted the library checkpoint, then the
repaired Fixture/account ownership and equivalent fault/ruin paths. The final independent scenario review found no blocking code finding.
Consumer review remains a separate gate.

This is provider publication preparation. Source full-SHA cutover, clean public
checkout and current whole-system/QEMU acceptance remain separate gates; these
host results do not prove physical reclamation or Stage 3/4 completion.

Clean public checkout at `5577e87d09fef2b4fd90093738dd97656b616d4b`:
29 direct host suites, 326 tests, locked/offline raw 0. Four host-suite lockfiles
were added after the first clean checkout correctly rejected missing locks.
The existing device-ruin request Mark is exported for the Source device-control
adapter without changing its protocol value; release RISC-V check raw 0.

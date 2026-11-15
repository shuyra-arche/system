# Native CI execution plan

`native.yml` keeps the acceptance workload intact and divides the longer guest
checks across independent jobs. The baseline run `43662800577` had six jobs:
`components`, one combined `guests` job, and four Kernel/System `handoff` jobs.
Recorded durations were 725 seconds for `guests`, 707 seconds for debug
Kernel/debug System handoff, 649 seconds for that handoff's native step, 645
seconds for debug Kernel/release System, 583 seconds for its native step, and
110–130 seconds for the two release Kernel handoffs. The guest steps included
runtime (20 seconds), entry (21), fatal/stall (31), release Loader (16),
release Kernel application jobs (31–37), and debug Loader (121); setup took
about 28 seconds.

The current plan has 17 effective jobs: components; one shared platform
producer; runtime, entry and fatal/stall; Loader for both Kernel profiles;
complete application for all four Kernel/System profile pairs; and eight
handoff jobs. The six debug Kernel handoff jobs each run one Hart count under a
`max-parallel` limit of six. The two release Kernel jobs each retain the full
1/2/4-Hart invocation. This changes scheduling, not test selection. Expected
critical path is about 5–7 minutes from the recorded timings; the added job
setup can raise total runner-minutes. Confirm these estimates against a new CI
run.

## Shared platforms

The producer builds these locked RV64 images once:

- Default-feature release platform for runtime/entry checks.
- `instructions` feature debug platform.
- `instructions` feature release platform.

The producer artifact records the source commit and tree, platform lock hash,
toolchain file hash, target, flags, feature/profile selections, exact `rustc
-Vv`, and SHA-256 hashes for all three ELFs. Consumers use the artifact ID
exported by the successful producer job, including when only failed jobs are
rerun. `download-artifact` uses `merge-multiple: true` so the contents land
directly in `tests/ci/target/platforms/`, where the verifier and guest runners expect them.
Consumers reject a mismatch before running any guest. Native evidence
artifacts include the full shared platform directory and metadata so the
actual input ELF and its provenance remain available with each result.

`tests/ci/action.yml` provides the shared Rust installation and cache. The
cache contains only Cargo registry/git data and rustup downloads, keyed by
runner OS, checked-in toolchain and lockfiles, plus the RV64 flags/features/
profiles. It does not contain Cargo target output or guest evidence. The local
`verify-platforms.sh` checks source identity, locks, toolchain, flags,
feature/profile metadata, compiler identity and all artifact hashes.

## Workload accounting

- Host contracts remain in `components` (89 checks in the baseline evidence).
- Complete application remains Kernel debug/release × System debug/release ×
  Hart 1/2/4: 36 executions.
- Handoff remains those four profiles × Hart 1/2/4 × recovery, authorization,
  cancellation and owner-death scenarios: 48 executions.
- Loader remains debug/release × Hart 1/2/4: 6 executions.
- Runtime and entry retain the existing normal, fatal-retirement and stalled
  bootstrap invocations and assertions, including the Kernel-admitted root
  exit requirement.

No workload was accepted based on a Complete event alone, and no target flags,
warnings, guest deadlines or selected dependencies were changed.

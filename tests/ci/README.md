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
1/2/4-Hart invocation. This changes scheduling, not test selection.

## Measured runs

Wall time is workflow start to completion; summed job time is the sum of each
job's started-to-completed duration. The 17-job runs reduced wall time, while
adding runner time. Neither new run is a qualified CI pass:

| Run | Wall time | Summed job time | Native evidence |
| --- | ---: | ---: | --- |
| [Baseline 43662800577](https://github.com/shuyra-arche/system/actions/runs/43662800577) | 12:09 | 41.08 runner-min | Successful: application 36/36, handoff 48/48, Loader 6/6, runtime/entry 6/6 guest invocations |
| [First split 43669366890](https://github.com/shuyra-arche/system/actions/runs/43669366890) | 7:21 | 45.25 runner-min | Native workload passed: application 36/36, handoff 48/48, Loader 6/6, runtime/entry 6/6; run failed in cache post-processing |
| [Cache fix 43670213070](https://github.com/shuyra-arche/system/actions/runs/43670213070) | 7:34 | 43.25 runner-min | Application 36/36, Loader 6/6 and runtime/entry 6/6 passed; handoff 45 passed, 1 authorization failure, 2 scenarios not run |

The last run is 37.7% faster by wall time and uses 5.3% more summed job time
than baseline. The handoff authorization failure and its runtime investigation
are tracked separately in [System #21](https://github.com/shuyra-arche/system/issues/21).
The logs for these runs contain no console-loss retry or inconclusive retry.
Do not treat either new run as completing full CI acceptance.

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

`tests/ci/action.yml` provides the shared Rust installation and cache. Before
the cache action starts, `cache-input-hash.sh` asks Git for only tracked
`rust-toolchain.toml` and `Cargo.lock` paths and hashes those files inside the
job container. The cache key uses that step output, runner OS, and the RV64
flags/features/profiles. It contains Cargo registry/git data and rustup's
installed toolchains; it does not contain Cargo target output or guest
evidence. Computing the digest from tracked inputs avoids traversing generated
`target/` directories when the cache action's post-job save runs. The local
`verify-platforms.sh` checks source identity, locks, toolchain, flags,
feature/profile metadata, compiler identity and all artifact hashes.

## Workload accounting

- Host contracts remain in `components` (89 checks in the baseline evidence).
- Complete application remains Kernel debug/release × System debug/release ×
  Hart 1/2/4: 36 executions.
- Handoff remains those four profiles × Hart 1/2/4 × recovery, authorization,
  cancellation, owner-death and three declaration-selected-owner scenarios: 84 executions (the original 48 retained).
- Loader remains debug/release × Hart 1/2/4: 6 executions.
- Runtime and entry retain the existing normal, fatal-retirement and stalled
  bootstrap invocations and assertions, including the Kernel-admitted root
  exit requirement.

No workload was accepted based on a Complete event alone, and no target flags,
warnings, guest deadlines or selected dependencies were changed.

For the Prepared-install increment, the eleven default handoff scenarios still run
all four Kernel/System profile pairs and 1/2/4 Harts (132 VMs), plus the three
release CPU contention VMs. Debug-Kernel matrix rows are split into lifecycle
(seven scenarios) and supply (provider lifecycle, source admission and two post-Prepared scenarios).
Release-Kernel rows retain all eleven. The 14 handoff jobs keep max-parallel 6 and
the existing 20-minute per-job bound, with suite-specific artifacts. This yields
23 required workflow jobs and 22 evidence artifacts/cache posts, without changing
any role/VM/startup deadline or behavioral/exit criterion. The prior two combined
double-debug jobs were cancelled at 20 minutes while their last VM was running;
completed scenarios were not failures, but those cancelled campaigns are not
accepted as full verification. Their logs remain in PR #27/CI 43793245104.

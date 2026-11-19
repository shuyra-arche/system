# System target scenarios

`runtime/`, `entry/`, `loader/`, `app/` and `handoff/` own the target guests,
fixtures and behavioral assertions. The Rust host CLI under
`server/tests/runner/` is the planned common execution entry; it owns process,
QEMU, output directory and reproducible receipt mechanics. CI selects and
triggers scenarios. It does not define scenario assertions.

## Migration status

This tree was moved from `tests/guest/`; its contract descriptions below are
retained during the runner migration. The CLI examples describe the candidate
interface and do not claim that the Rust runner or old/new parity has passed.
Keep `.github/workflows/native.yml` and its existing checks until candidate
checks have been run and shown equivalent in scenario coverage, evidence,
timeout, exit and retirement criteria. Do not infer full System/Toolchain or
hardware acceptance from a host-runner invocation. Cross-provider product
acceptance remains with Vanllia #39.

## CLI

Run from the repository root. `PATH` must name the selected, qualified Kernel
platform ELF. Set `QEMU_RISCV64` and `QEMU_OPENSBI` to the QEMU executable and
OpenSBI firmware. `NEW` must be a fresh evidence output directory; preserve
failed receipts rather than reusing it for another attempt. The runner runs on
the host target shown below; the guests remain RV64 `no_std` binaries.

```sh
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- runtime --kernel PATH --output NEW --harts 1 4
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- entry --kernel PATH --output NEW --harts 1
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- loader --kernel PATH --output NEW --profile release --kernel-profile release --harts 1 2 4
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- application --kernel PATH --output NEW --profile release --kernel-profile release --harts 1 2 4
cargo run --locked --manifest-path server/tests/runner/Cargo.toml --target x86_64-unknown-linux-gnu -- handoff --kernel PATH --output NEW --profile release --kernel-profile release --harts 1 2 4
```

Use `--profile debug|release` for the Server/fixture profile and
`--kernel-profile debug|release` for the supplied Kernel profile; pass matching
artifacts and profile metadata. Entry's mutually exclusive fault modes are
`--abort` and `--stall`. `handoff` accepts `--scenarios <names...>` for a
focused subset; omitting it selects the full default campaign. Supply
`--harts 1 2 4` only with a platform explicitly qualified for all-Hart
instruction publication. The runtime data-only fixture supports Harts 1 and 4
and does not qualify executable publication.

Each scenario README names its owner contract, required outcome, profile/Hart
matrix, and timeout. A `PASS` string is necessary only where specified; typed
events, actual Kernel-admitted exit, physical retirement and zero process exit
remain separate checks. A timeout, fault, nonzero exit, missing event or failed
retirement is a failure. Handoff may qualify one fresh VM only after all
behavioral assertions pass and the checker reports positive Kernel
`console-lost`; first-attempt evidence remains inconclusive and a second
inconclusive attempt fails. No other failure is retried or accepted.

## Frozen providers and scope

The current target manifests and locks select Kernel
`789ffd19b5359f3de172fb9aad8a61f6a7113a64` (ABI/Ledger 13/13, Capsule 2),
Twine `932454c07bd2673ac7d6d9612d36182aae71e8b2`, Weave
`7a7d67fb3f190aeb43049d051b0972bcd53213bc`, and Unit schema provider
`aaa2b58c1ef0b40428dcc71d1321736156551109`. The qualified platform uses
Vanllia `0eb086a2416eee8921416b12bf94819b30b6bb1f`. System API/Client remain
protocol v4. Use the Cargo manifests/locks and runner receipts as the exact
execution provenance.

These CPU-only QEMU scenarios cover selected System component and composition
contracts. They do not prove device retirement, physical hardware, all public
Instance/UnitRoot contracts, a Toolchain `std` product Unit, or full
System/Toolchain lifecycle acceptance. A native component pass is not full
product acceptance.

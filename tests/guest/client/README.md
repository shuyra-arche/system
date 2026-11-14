# Native System component guest

Run from the System repository root with the RV64 target, Kernel 10 QEMU image,
QEMU system RISC-V and identified OpenSBI installed:

```sh
RUSTUP_TOOLCHAIN=nightly python3 tests/guest/client/run.py /path/to/shuyra
```

The runner builds `image/` as an independent ordinary User ELF, passes its exact
artifact through `SYSTEM_LOADER_IMAGE`, builds this guest, then boots one/four
Harts with a 60-second process bound. Every required typed PASS marker must be
present; a panic marker or nonzero process result fails the run. The guest has a
bounded fixture heap and does not replace the assembled System executable.

For target Clippy, prepare the same actual image explicitly:

```sh
cargo +nightly build --locked --release --manifest-path tests/guest/client/image/Cargo.toml --target riscv64gc-unknown-none-elf
SYSTEM_LOADER_IMAGE="$PWD/tests/guest/client/image/target/riscv64gc-unknown-none-elf/release/system-loader-image" cargo +nightly clippy --locked --manifest-path tests/guest/client/Cargo.toml --target riscv64gc-unknown-none-elf -- -D warnings
```

The guest reuses the production ingress, Identity/Operator, worker/bootstrap and
Loader code. Its linker exposes the immutable RX span at a reserved high VA for
the Loader's target-side initializer. The ordinary image has independent low
VAs and private DATA/BSS. This confirms component behavior, not full product
boot, Toolchain `std`, hardware or a completely clean Server dependency closure.

The cross-Team probe exercises actual C/S/U identities. C -> S and U -> C
remain valid. S grants an initial request queue to U while U is Dormant;
S cannot Resume U. After C resumes U, U grants its own reply queue to S and
sends its recipient-local coordinate through that initial queue. S uses the
Kernel-reported sender in Sender::reply; a mismatched requester is rejected.
U checks the actual S reply identity and value 42. S stays live until C observes
U's reclaimed result; S then exits 43 and is reclaimed. Explicit mapping and
Whole Nest roots are retained and returned. One/four-Hart runs require both
reply-cross-team and grant-cross-team success markers. The fixed registration
limits are supplied by Kernel; no identity or creator is forged.

Validated component inputs: Kernel/API/Client
`6d78ff68b95602deac2480a5addf7e693fd88fcc`, Twine
`37c75f5bbb0794b4e9daf61bf0a93e81492e3daf`, Rust nightly-2026-11-13
(`8eb778d04094d3af4b26580cab327e0413eb1e02`), QEMU 10.0.13 and OpenSBI 1.6.
The guest uses normal locked Git dependencies; no local SDK patch is needed.
Production Server/Support SDK pins are aligned to the same provider. The legacy
Server application (including runtime heap Cell construction) and old fixtures
still need migration. Their full build is not passing release evidence.

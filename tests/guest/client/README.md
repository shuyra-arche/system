# Native System component guest

Run from the System repository root with the RV64 target, Kernel 9 QEMU image,
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

The adjacency probe now checks both direct-grant directions while the foreign
User Task and sibling System worker are live. Creator C -> sibling S succeeds;
User U -> creator C succeeds; U -> S and S -> U both return exact PendFail::Denied
under ABI9. S stays alive until U completes; results 42/43 and reclaimed receipts
are verified before deletion. The runner requires the additional
`PASS grant-adjacency worker-to-unit-denied=1` marker on one/four Harts. This is
also a normal Unit activation blocker for worker-issued grants, not solely a
reply-queue issue. It does not establish a replacement admission contract for
Kernel #140 or justify weakening recipient protection.

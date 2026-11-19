#!/usr/bin/env bash
set -euo pipefail

platform_dir="${1:-tests/ci/target/platforms}"
platform_lock="${2:-tests/guest/runtime/platform/Cargo.lock}"
manifest="$platform_dir/manifest.txt"
test -f "$manifest"
test -f "$platform_lock"
test -f "$platform_dir/rustc-vv.txt"
test -f "$platform_dir/SHA256SUMS"

manifest_value() {
  sed -n "s/^$1=//p" "$manifest"
}

test "$(manifest_value revision)" = "$(git rev-parse HEAD)"
test "$(manifest_value tree)" = "$(git rev-parse 'HEAD^{tree}')"
test "$(manifest_value platform-lock-sha256)" = "$(sha256sum "$platform_lock" | cut -d' ' -f1)"
test "$(manifest_value toolchain-sha256)" = "$(sha256sum rust-toolchain.toml | cut -d' ' -f1)"
grep -Fx 'target=riscv64gc-unknown-none-elf' "$manifest"
grep -Fx "flags=${RUSTFLAGS:?RUSTFLAGS must be set}" "$manifest"
grep -Fx 'runtime-feature=default;profile=release' "$manifest"
grep -Fx 'instructions-feature=instructions;profiles=debug,release' "$manifest"
rustc -Vv | cmp - "$platform_dir/rustc-vv.txt"
test "$(sha256sum "$platform_dir/rustc-vv.txt" | cut -d' ' -f1)" = "$(cat "$platform_dir/rustc-vv.sha256")"
(cd "$platform_dir" && sha256sum --check SHA256SUMS)

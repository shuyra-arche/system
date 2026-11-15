#!/usr/bin/env bash
set -euo pipefail

: "${GITHUB_WORKSPACE:?GITHUB_WORKSPACE must identify the checked-out repository}"
: "${GITHUB_OUTPUT:?GITHUB_OUTPUT must identify the current step output file}"

input_hash="$({
  git -c safe.directory="$GITHUB_WORKSPACE" ls-files -z -- \
    rust-toolchain.toml ':(glob)**/Cargo.lock' |
    xargs -0 sha256sum --
} | sha256sum | cut -d' ' -f1)"
test -n "$input_hash"
printf 'hash=%s\n' "$input_hash" >> "$GITHUB_OUTPUT"

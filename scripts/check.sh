#!/usr/bin/env bash
set -euo pipefail

# cargo fmt covers every member only from the workspace root.
cd "$(dirname "$0")/.."

echo "==> cargo fmt --check"
# Every workspace member. Not `--all`: it would format the vendored gpui too.
cargo fmt -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> gpui isolation check (every crate below app must not depend on gpui, any dependency kind, any target)"
# Every member but `app`. Each flag and the here-string are load-bearing; see docs/tooling.md.
below_app=(--workspace --exclude inkra-app -e normal,build,dev --target=all --all-features)
tree_output=$(cargo tree "${below_app[@]}")
if grep -qE '(^|[[:space:]])gpui v[0-9]' <<< "$tree_output"; then
  echo "ERROR: a crate below app depends on gpui — it must stay gpui-free:" >&2
  cargo tree "${below_app[@]}" -i gpui >&2
  exit 1
fi

echo "==> cargo deny check"
# Not installed by default, so fail with the install command.
if ! command -v cargo-deny >/dev/null 2>&1; then
  echo "ERROR: cargo-deny is not installed — run: cargo install --locked cargo-deny" >&2
  exit 1
fi
cargo deny check

echo "==> cargo test"
cargo test --workspace

echo "==> cargo build --release"
cargo build --workspace --release

echo "==> all checks passed"

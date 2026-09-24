#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo fmt --check"
# Not `--all`: it would format the vendored gpui too.
cargo fmt -p inkra-text -p inkra-editor -p inkra-app -p inkra-vault -p inkra-markdown -p inkra-index -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> gpui isolation check (every crate below app must not depend on gpui, any dependency kind, any target)"
# Every crate below `app`. Each flag and the here-string are load-bearing; see docs/tooling.md.
for crate in inkra-text inkra-editor inkra-vault inkra-markdown inkra-index; do
  tree_output=$(cargo tree -p "$crate" -e normal,build,dev --target=all --all-features)
  if grep -qE '(^|[[:space:]])gpui v[0-9]' <<< "$tree_output"; then
    echo "ERROR: $crate depends on gpui — it must stay gpui-free" >&2
    exit 1
  fi
done

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

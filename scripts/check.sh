#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> gpui isolation check (core must not depend on gpui)"
if cargo tree -p gpui-test-core -e normal | grep -qE '(^|[[:space:]])gpui v[0-9]'; then
  echo "ERROR: gpui-test-core depends on gpui — core must stay gpui-free" >&2
  exit 1
fi

echo "==> cargo test"
cargo test --workspace

echo "==> cargo build --release"
cargo build --workspace --release

echo "==> all checks passed"

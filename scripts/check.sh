#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> gpui isolation check (core must not depend on gpui, in any dependency kind)"
# Captured into a variable (not piped directly into grep) so a `cargo tree`
# failure itself trips `set -e` instead of being masked by grep's own exit
# status. Checks normal, build, and dev edges: a `gpui` dev-dependency on
# core would defeat the point of keeping core gpui-free (headless, fast
# tests) just as much as a normal one would.
tree_output=$(cargo tree -p gpui-test-core -e normal,build,dev)
if echo "$tree_output" | grep -qE '(^|[[:space:]])gpui v[0-9]'; then
  echo "ERROR: gpui-test-core depends on gpui — core must stay gpui-free" >&2
  exit 1
fi

echo "==> cargo test"
cargo test --workspace

echo "==> cargo build --release"
cargo build --workspace --release

echo "==> all checks passed"

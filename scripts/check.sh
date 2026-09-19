#!/usr/bin/env bash
set -euo pipefail

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> gpui isolation check (core must not depend on gpui, any dependency kind, any target)"
# Captured into a variable (not piped directly into grep) so a `cargo tree`
# failure itself trips `set -e` instead of being masked by grep's own exit
# status. Checks normal, build, and dev edges: a `gpui` dev-dependency on
# core would defeat the point of keeping core gpui-free (headless, fast
# tests) just as much as a normal one would. --target=all covers
# platform-gated dependencies (e.g. `[target.'cfg(windows)'.dependencies]`),
# which `cargo tree` otherwise silently omits for platforms other than the
# host. The match uses a here-string, not `echo ... | grep`: piping a large
# tree into `grep -q` lets grep exit right after its first match, closing
# the pipe while `echo` is still writing — the resulting SIGPIPE (128+13)
# outranks grep's real (successful) exit code under `pipefail` and would
# silently swallow a genuine match.
tree_output=$(cargo tree -p gpui-test-core -e normal,build,dev --target=all)
if grep -qE '(^|[[:space:]])gpui v[0-9]' <<< "$tree_output"; then
  echo "ERROR: gpui-test-core depends on gpui — core must stay gpui-free" >&2
  exit 1
fi

echo "==> cargo test"
cargo test --workspace

echo "==> cargo build --release"
cargo build --workspace --release

echo "==> all checks passed"

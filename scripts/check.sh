#!/usr/bin/env bash
set -euo pipefail

# cargo fmt covers every member only from the workspace root.
cd "$(dirname "$0")/.."

echo "==> cargo fmt --check"
# Every workspace member.
cargo fmt -- --check

echo "==> typos"
# Not installed by default, so fail with the install command.
if ! command -v typos >/dev/null 2>&1; then
  echo "ERROR: typos is not installed — run: cargo install --locked typos-cli" >&2
  exit 1
fi
typos

echo "==> crate graph check"
# The edges CLAUDE.md's Architecture allows, any dependency kind. A new edge fails until listed here.
allowed_edges="inkra-editor inkra-text
inkra-markdown inkra-text
inkra-index inkra-vault
inkra-index inkra-markdown
inkra-index inkra-text
inkra-app inkra-text
inkra-app inkra-editor
inkra-app inkra-vault
inkra-app inkra-markdown
inkra-app inkra-index
inkra-app inkra-tabs
inkra-app inkra-layout"
if ! command -v jq >/dev/null 2>&1; then
  echo "ERROR: jq is not installed — run: brew install jq" >&2
  exit 1
fi
edges=$(cargo metadata --format-version 1 --no-deps |
  jq -r '.packages[] | .name as $from | .dependencies[] | select(.path != null and .name != $from) | "\($from) \(.name)"' |
  sort -u)
if forbidden=$(grep -vxF "$allowed_edges" <<< "$edges"); then
  echo "ERROR: a workspace dependency CLAUDE.md's Architecture does not allow (crate, dependency):" >&2
  echo "$forbidden" >&2
  exit 1
fi

echo "==> lint inheritance check"
# A member without `[lints] workspace = true` escapes every workspace lint; a tracked rustflag can silence one.
manifests=$(cargo metadata --format-version 1 --no-deps | jq -r '.packages[].manifest_path')
while IFS= read -r manifest; do
  if ! awk '/^\[/ { section = $0 } section == "[lints]" && /^workspace *= *true/ { found = 1 } END { exit !found }' "$manifest"; then
    echo "ERROR: $manifest has no [lints] workspace = true" >&2
    exit 1
  fi
done <<< "$manifests"
# --untracked: a config not yet added overrides lints locally all the same.
if flags=$(git grep --untracked -nE 'rustflags|RUSTFLAGS' -- ':(glob)**/.cargo/**' '.github/' ':(exclude).claude/worktrees'); then
  echo "ERROR: rustflags in a cargo or CI config bypass the workspace lints; set lints in Cargo.toml:" >&2
  echo "$flags" >&2
  exit 1
fi

echo "==> ast-grep"
# The structural rules in .ast-grep/rules/. Not installed by default, so fail with the install command.
if ! command -v ast-grep >/dev/null 2>&1; then
  echo "ERROR: ast-grep is not installed — run: cargo install --locked ast-grep" >&2
  exit 1
fi
ast-grep scan
# A suppression comment silences any rule; narrow the rule's `ignores` instead, where review sees it.
if suppressed=$(grep -rn 'ast-grep-ignore' crates); then
  echo "ERROR: an ast-grep-ignore comment — fix the code or the rule:" >&2
  echo "$suppressed" >&2
  exit 1
fi

echo "==> line ceilings"
./scripts/line-ceilings.sh

echo "==> cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "==> clock check (library code below app)"
# Clippy resolves names, so an alias or `.elapsed()` cannot hide a clock read. --lib --bins leaves tests and benches out.
CLIPPY_CONF_DIR="$PWD/scripts/clippy/below-app" \
  cargo clippy --workspace --exclude inkra-app --lib --bins --all-features -- -D clippy::disallowed_methods

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

echo "==> cargo test, inkra-index in release"
# Release has no overflow checks: the decode must reject without them.
cargo test -p inkra-index --release

echo "==> cargo build --release"
cargo build --workspace --release

echo "==> all checks passed"

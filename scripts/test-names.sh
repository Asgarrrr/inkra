#!/usr/bin/env bash
# Prints every test in the workspace by its leaf name, sorted, one per line,
# duplicates kept. A refactor that moves tests between files changes their
# module path but not their leaf — `#[gpui::test]` keeps the outer identifier
# (vendor/zed-reference/crates/gpui_macros/src/test.rs) — so diffing two runs
# of this script shows exactly the tests added or dropped. A forgotten `mod`
# line drops a whole file's tests and still compiles; this is what catches it.
set -euo pipefail

list=$(cargo test --workspace --quiet -- --list 2>/dev/null)
grep -E ': test$' <<< "$list" | sed -E 's/: test$//; s/.*:://' | sort

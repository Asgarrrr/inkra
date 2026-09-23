#!/usr/bin/env bash
# Leaf test names, sorted; diff two runs to catch tests a split dropped (docs/tooling.md).
set -euo pipefail

list=$(cargo test --workspace --quiet -- --list 2>/dev/null)
grep -E ': test$' <<< "$list" | sed -E 's/: test$//; s/.*:://' | sort

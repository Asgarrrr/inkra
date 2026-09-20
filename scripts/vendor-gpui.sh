#!/usr/bin/env bash
set -euo pipefail

# Restores vendor/zed-reference, the gpui source `crates/app/Cargo.toml` builds
# against. `vendor/` is gitignored, so a fresh clone of this repo has no gpui
# and will not build until this runs.
#
# Pinned by commit, never by branch: the whole point of vendoring is that the
# source we read and the source we compile are the same bytes. Bumping means
# editing REV here, re-running, and re-checking every `file:line` citation in
# docs/ — that is the cost, and it is deliberate.

REV=526c95d474b2bf800e95aab30222fa0341f08e19
REPO=https://github.com/zed-industries/zed.git
DEST="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/vendor/zed-reference"

# Cargo needs every workspace member of the vendored workspace to exist on disk,
# not just gpui's own dependencies — `extensions/` and `tooling/` are members
# too, and a missing one fails resolution for the whole build.
SPARSE=(crates tooling extensions)

if [ -d "$DEST/.git" ]; then
  echo "==> $DEST exists, fetching $REV"
  git -C "$DEST" fetch --filter=blob:none origin "$REV"
else
  echo "==> cloning zed into $DEST"
  mkdir -p "$(dirname "$DEST")"
  git clone --filter=blob:none --sparse "$REPO" "$DEST"
fi

git -C "$DEST" sparse-checkout set "${SPARSE[@]}"
git -C "$DEST" checkout --detach "$REV"

echo "==> vendored zed at $(git -C "$DEST" rev-parse --short HEAD)"

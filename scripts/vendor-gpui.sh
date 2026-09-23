#!/usr/bin/env bash
set -euo pipefail

# Restores the gitignored gpui checkout, pinned by commit; bumping it is in docs/tooling.md.

REV=526c95d474b2bf800e95aab30222fa0341f08e19
REPO=https://github.com/zed-industries/zed.git
DEST="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/vendor/zed-reference"

# Cargo resolution needs every member of zed's workspace on disk, not just gpui's.
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

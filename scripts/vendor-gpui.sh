#!/usr/bin/env bash
set -euo pipefail

# Checks out Zed, for reading, at the rev cargo builds gpui from; bumping it is in docs/tooling.md.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REV=$(sed -nE 's|^source = "git\+https://github.com/zed-industries/zed\?rev=([0-9a-f]{40})#.*|\1|p' "$ROOT/Cargo.lock" | sort -u)
if [ "$(wc -l <<< "$REV")" -ne 1 ] || [ -z "$REV" ]; then
  echo "expected one zed rev in Cargo.lock, found: ${REV:-none}" >&2
  exit 1
fi
REPO=https://github.com/zed-industries/zed.git
DEST="$ROOT/vendor/zed-reference"

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

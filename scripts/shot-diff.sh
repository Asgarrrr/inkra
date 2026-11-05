#!/usr/bin/env bash
# Renders every shot scene and compares two renders byte for byte: the pixel gate of a migration.
# usage: scripts/shot-diff.sh snapshot <dir>
#        scripts/shot-diff.sh compare <before> <after> [allowed-stem...]
set -euo pipefail
shopt -s nullglob
cd "$(dirname "$0")/.."

usage() {
  echo "usage: $0 snapshot <dir> | compare <before> <after> [allowed-stem...]" >&2
  exit 2
}

snapshot() {
  local dir=$1
  mkdir -p "$dir"
  if [ -n "$(ls -A "$dir")" ]; then
    echo "ERROR: $dir is not empty" >&2
    exit 1
  fi
  local marker
  marker=$(mktemp)
  # A shot older than the marker is a leftover of an earlier run, not this render.
  touch "$marker"
  cargo run -p inkra-app --features shot --example shot
  local copied=0 png
  while IFS= read -r png; do
    cp "$png" "$dir/"
    copied=$((copied + 1))
  done < <(find target/shots -maxdepth 1 -name '*.png' -newer "$marker")
  rm -f "$marker"
  if ((copied == 0)); then
    echo "ERROR: the run wrote no shot" >&2
    exit 1
  fi
  echo "==> $copied shots in $dir"
}

compare() {
  local before=$1 after=$2
  shift 2
  local allowed=" $* "
  local same=0 diff=0 added=0 missing=0 failed=0 png stem
  for png in "$before"/*.png; do
    stem=$(basename "$png" .png)
    if [ ! -f "$after/$stem.png" ]; then
      echo "MISSING $stem"
      missing=$((missing + 1))
      failed=1
    elif cmp -s "$png" "$after/$stem.png"; then
      same=$((same + 1))
    elif [[ $allowed == *" $stem "* ]]; then
      echo "DIFF $stem (allowed)"
      diff=$((diff + 1))
    else
      echo "DIFF $stem"
      diff=$((diff + 1))
      failed=1
    fi
  done
  for png in "$after"/*.png; do
    stem=$(basename "$png" .png)
    if [ ! -f "$before/$stem.png" ]; then
      echo "ADDED $stem"
      added=$((added + 1))
    fi
  done
  echo "$same same, $diff diff, $added added, $missing missing"
  exit "$failed"
}

(($# >= 1)) || usage
case $1 in
  snapshot) (($# == 2)) || usage; snapshot "$2" ;;
  compare) (($# >= 3)) || usage; compare "${@:2}" ;;
  *) usage ;;
esac

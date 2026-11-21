#!/usr/bin/env bash
# The duplication ratchet: no 6-line run of source logic may appear more often than at the branch base (docs/tooling.md).
# usage: scripts/duplication.sh [base]   (default: the merge base with origin/main, else main)
set -euo pipefail
cd "$(dirname "$0")/.."

base=${1:-}
if [ -z "$base" ] && ! base=$(git merge-base HEAD origin/main 2>/dev/null || git merge-base HEAD main 2>/dev/null); then
  echo "ERROR: no merge base with origin/main or main — pass a base: $0 <base>" >&2
  exit 1
fi

# Tests and benches are exempt: a fixture repeated per case reads better than a helper.
source_files() {
  grep -E '^crates/.*\.rs$' | grep -vE '(^|/)(tests?|benches)(/|\.rs$)|(^|/)tests?_[^/]*\.rs$|_tests?\.rs$' || true
}

tmp=$(mktemp -d "${TMPDIR:-/tmp}/duplication.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

base_files=$(git ls-tree -r --name-only "$base" -- crates | source_files)
if [ -n "$base_files" ]; then
  # shellcheck disable=SC2086 # paths under crates/ hold no spaces
  git archive "$base" -- $base_files | tar -x -C "$tmp"
fi
# sort -u: a path in a merge conflict is listed once per stage.
current_files=$(git ls-files --cached --others --exclude-standard -- crates | sort -u | source_files |
  while IFS= read -r path; do [ ! -f "$path" ] || echo "$path"; done)

# A window counts when it repeats, repeats more than at the base, and holds 3 statements:
# runs of braces, field lists and match arms repeat by nature and say nothing about copied logic.
# shellcheck disable=SC2086
ranges=$(awk -v base="$tmp/" '
  function flush(   i, j, key, stmts) {
    for (i = 1; i + 5 <= n; i++) {
      key = line[i]; stmts = stmt[i]
      for (j = i + 1; j <= i + 5; j++) { key = key SUBSEP line[j]; stmts += stmt[j] }
      if (in_base) { before[key]++; continue }
      count[key]++; statements[key] = stmts
      where[key] = where[key] file ":" at[i] ":" at[i + 5] "\n"
    }
    n = 0
  }
  FNR == 1 {
    flush()
    in_base = index(FILENAME, base) == 1
    file = in_base ? substr(FILENAME, length(base) + 1) : FILENAME
  }
  {
    text = $0
    gsub(/^[ \t]+|[ \t]+$/, "", text)
    if (text == "" || text ~ /^\/\// || text ~ /^use / || text ~ /^[ \t{}()\[\];,]*$/) next
    n++; line[n] = text; at[n] = FNR
    stmt[n] = text ~ /(^|[^A-Za-z0-9_])(let|if|match|for|while|return)([^A-Za-z0-9_]|$)|[^=!<>]=[^=>]|;$/
  }
  END {
    flush()
    for (key in count) {
      floor = before[key] > 1 ? before[key] : 1
      if (count[key] > floor && statements[key] >= 3) printf "%s", where[key]
    }
  }
' $(sed "s|^|$tmp/|" <<< "$base_files") $current_files)

[ -z "$ranges" ] && exit 0

echo "ERROR: logic repeated since $(git rev-parse --short "$base"); extract the shared part instead of copying it:" >&2
# Overlapping windows of one copy merge into a single range per file.
sort -t: -k1,1 -k2,2n <<< "$ranges" | awk -F: '
  $1 == file && $2 <= last + 1 { if ($3 > last) last = $3; next }
  { if (file != "") print "  " file ":" first "-" last; file = $1; first = $2; last = $3 }
  END { print "  " file ":" first "-" last }
' >&2
exit 1

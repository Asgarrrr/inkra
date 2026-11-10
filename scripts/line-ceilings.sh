#!/usr/bin/env bash
# The file-length ratchet: a listed source file may shrink but never grow, an unlisted one stays within 400 lines (docs/tooling.md).
# usage: scripts/line-ceilings.sh [--lower]
set -euo pipefail
cd "$(dirname "$0")/.."

limit=400
table=scripts/line-ceilings.tsv
lower_command="./scripts/line-ceilings.sh --lower"

# Test files are exempt: a new test must not force a split; the PostToolUse hook still warns past 400.
source_files() {
  find crates -name '*.rs' -type f | grep -vE '(^|/)(tests|[^/]*_tests)\.rs$|/tests/' | sort
}

lines_of() {
  wc -l < "$1" | tr -d ' '
}

check() {
  local failed=0 path ceiling lines listed
  while IFS=$'\t' read -r path ceiling || [ -n "$path" ]; do
    [ -n "$path" ] || continue
    case "$ceiling" in
      '' | *[!0-9]*)
        echo "ERROR: $table: no line count after $path" >&2
        exit 1
        ;;
    esac
    if [ ! -f "$path" ]; then
      echo "ERROR: $path is listed but gone — run: $lower_command" >&2
      failed=1
      continue
    fi
    lines=$(lines_of "$path")
    if [ "$lines" -gt "$ceiling" ]; then
      echo "ERROR: $path has $lines lines, over its ceiling of $ceiling — split it by concern" >&2
      failed=1
    elif [ "$lines" -lt "$ceiling" ]; then
      echo "ERROR: $path has $lines lines, under its ceiling of $ceiling — run: $lower_command" >&2
      failed=1
    fi
  done < "$table"

  listed=$(cut -f1 "$table")
  while IFS= read -r path; do
    lines=$(lines_of "$path")
    # A here-string, not a pipe: see the gpui isolation check in docs/tooling.md.
    if [ "$lines" -gt "$limit" ] && ! grep -qxF "$path" <<< "$listed"; then
      echo "ERROR: $path has $lines lines, past $limit and not in $table — split it by concern" >&2
      failed=1
    fi
  done < <(source_files)

  return "$failed"
}

lower() {
  local out path ceiling lines
  out=$(mktemp "${TMPDIR:-/tmp}/line-ceilings.XXXXXX")
  while IFS=$'\t' read -r path ceiling || [ -n "$path" ]; do
    [ -n "$path" ] || continue
    case "$ceiling" in
      '' | *[!0-9]*)
        echo "ERROR: $table: no line count after $path" >&2
        exit 1
        ;;
    esac
    if [ ! -f "$path" ]; then
      echo "dropped $path: gone"
      continue
    fi
    lines=$(lines_of "$path")
    if [ "$lines" -le "$limit" ]; then
      echo "dropped $path: $lines lines"
      continue
    fi
    if [ "$lines" -lt "$ceiling" ]; then
      echo "lowered $path: $ceiling -> $lines"
      ceiling=$lines
    fi
    printf '%s\t%s\n' "$path" "$ceiling" >> "$out"
  done < "$table"
  # `cat >` keeps the table's mode; `mv` would give it mktemp's 0600.
  cat "$out" > "$table"
  rm "$out"
}

case "${1:-}" in
  "") check ;;
  --lower) lower ;;
  *)
    echo "usage: $0 [--lower]" >&2
    exit 2
    ;;
esac

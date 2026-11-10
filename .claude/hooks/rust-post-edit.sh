#!/usr/bin/env bash
# PostToolUse hook (Write|Edit): format the touched .rs file, then surface
# `cargo check` diagnostics for the workspace back to the model immediately,
# instead of waiting for the next manual build.
set -uo pipefail

input=$(cat)
file=$(echo "$input" | jq -r '.tool_input.file_path // .tool_response.filePath // empty')

case "$file" in
  *.rs) ;;
  *) exit 0 ;;
esac

[ -f "$file" ] || exit 0

# Physical paths on both sides: git prints /private/var for a file given as /var under $TMPDIR.
file="$(cd "$(dirname "$file")" && pwd -P)/$(basename "$file")"
repo_root="$(cd "$(dirname "$file")" && git rev-parse --show-toplevel 2>/dev/null)"
[ -n "$repo_root" ] || exit 0

case "$file" in
  "$repo_root"/vendor/*) exit 0 ;;
esac

# From the repo root, so rustup picks rust-toolchain.toml's rustfmt even when
# the session sits in vendor/zed-reference, which pins another toolchain.
(cd "$repo_root" && rustfmt --edition 2024 "$file" 2>/dev/null)

check_output=$(cd "$repo_root" && cargo check --workspace --message-format=short 2>&1)
check_status=$?

context=""
if [ "$check_status" -ne 0 ]; then
  context="cargo check failed after this edit:
$(printf '%s' "$check_output" | tail -100)"
fi

# CLAUDE.md: split a file past ~400 lines. A warning, so it never blocks an edit.
lines=$(wc -l < "$file" | tr -d ' ')
if [ "$lines" -gt 400 ]; then
  context="${context:+$context

}${file#"$repo_root"/} has $lines lines, past the ~400 of CLAUDE.md. Unless it is a test file, check.sh's line ceilings fail it when it grows past its entry in scripts/line-ceilings.tsv, or past 400 when unlisted. If this change adds a subject to it, split by concern (sidebar/{tree,edit,render}.rs); do not split as a drive-by, and run scripts/test-names.sh around a test file move."
fi

if [ -n "$context" ]; then
  jq -n --arg ctx "$context" \
    '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $ctx}}'
fi

exit 0

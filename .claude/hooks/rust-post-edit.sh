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

if [ "$check_status" -ne 0 ]; then
  context=$(printf '%s' "$check_output" | tail -100)
  jq -n --arg ctx "$context" \
    '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: ("cargo check failed after this edit:\n" + $ctx)}}'
fi

exit 0

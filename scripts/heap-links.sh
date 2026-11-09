#!/usr/bin/env bash
# usage: heap-links.sh [--home DIR] [--settle S] <binary> <vault>
# The launch line, then the live heap the links hold at idle, warm: what `malloc_history`
# lists under `persist::notes::decode`, and what `Seed::build` allocates itself.
# --home DIR starts the binary with HOME=DIR, a fresh temporary folder by default so the user's
# session and cache stay untouched; --settle S waits S seconds after the launch line (10).
usage="usage: heap-links.sh [--home DIR] [--settle S] <binary> <vault>"
home=; settle=10
while case $1 in
  --home) [ $# -ge 2 ] || { echo "$usage" >&2; exit 2; }; home=$2; shift 2 ;;
  --settle) [ $# -ge 2 ] || { echo "$usage" >&2; exit 2; }; settle=$2; shift 2 ;;
  --*) echo "$usage" >&2; exit 2 ;;
  *) false ;;
esac; do :; done
[ $# -eq 2 ] || { echo "$usage" >&2; exit 2; }
bin=$1; vault=$2
[ -d "$vault" ] || { echo "ERROR: no vault at $vault" >&2; exit 2; }
if [ -z "$home" ]; then home=$(mktemp -d); trap 'rm -rf "$home"' EXIT; fi
mkdir -p "$home"

# The app's key: FNV-1a 64 of the vault's lexical absolute path (app_files.rs).
key() {
  local h=$(( 0xcbf29ce484222325 )) b
  for b in $(printf %s "$1" | od -An -tu1 -v); do h=$(( (h ^ b) * 0x100000001b3 )); done
  printf %016x "$h"
}
cache=$home/Library/Caches/Inkra/index/$(key "$(cd "$vault" && pwd -L)").idx
idx() { [ -f "$cache" ] && echo "$cache"; }

if [ -z "$(idx)" ]; then
  HOME="$home" "$bin" "$vault" 2> /dev/null & pid=$!
  for _ in $(seq 1 180); do [ -n "$(idx)" ] && break; sleep 1; done
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  [ -n "$(idx)" ] || { echo "ERROR: no index cache at $cache after 180 s" >&2; exit 1; }
fi

log=$(mktemp); report=$(mktemp)
HOME="$home" MallocStackLogging=lite "$bin" "$vault" 2> "$log" & pid=$!
for _ in $(seq 1 300); do grep -q '^launch:' "$log" && break; sleep 0.1; done
sleep "$settle"
malloc_history "$pid" -allBySize > "$report" 2> /dev/null
kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null

# A stack under `decode` counts there whatever it calls. A stack counts for the seed only when
# its innermost `inkra_index` frame is `Seed::build`: its callees (`Stems`, `Corpus`) are not links.
# A release build may inline `Seed::build` into `BuildJob::run`, which allocates nothing of its own.
heap=$(awk '
  /^[0-9]+ calls? for [0-9]+ bytes: / {
    if (index($0, "persist::notes::decode")) { decode += $4; next }
    n = split($0, frames, " \\| ")
    for (i = n; i > 0; i--) if (frames[i] ~ /inkra_index(::|\.\.)/) break
    if (i > 0 && frames[i] ~ /Seed::build|BuildJob::run/) seed += $4
  }
  END {
    mib = 1048576
    printf "decode %.1f MiB | seed %.1f MiB | total %.1f MiB", decode / mib, seed / mib, (decode + seed) / mib
  }' "$report")
echo "$(grep '^launch:' "$log" || echo 'no launch line') | $heap | idx $(stat -f %z "$(idx)") B"
rm -f "$log" "$report"

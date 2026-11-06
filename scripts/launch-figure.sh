#!/usr/bin/env bash
# usage: launch-figure.sh [--home DIR] [--cold] <binary> <vault> <runs>
# The launch line, then the process RSS in kB and phys_footprint in bytes once it is printed.
# --home DIR starts the binary with HOME=DIR; --cold runs `sudo -n purge` before each run.
usage="usage: launch-figure.sh [--home DIR] [--cold] <binary> <vault> <runs>"
home=; cold=
while case $1 in
  --home) [ $# -ge 2 ] || { echo "$usage" >&2; exit 2; }; home=$2; shift 2 ;;
  --cold) cold=1; shift ;;
  --*) echo "$usage" >&2; exit 2 ;;
  *) false ;;
esac; do :; done
bin=$1; vault=$2; runs=${3:-3}
if [ -n "$cold" ] && ! sudo -n purge 2>/dev/null; then
  echo "ERROR: --cold needs passwordless sudo purge; run it yourself" >&2
  exit 2
fi
[ -n "$home" ] && mkdir -p "$home"
for i in $(seq 1 "$runs"); do
  [ -n "$cold" ] && sudo -n purge
  log=$(mktemp)
  if [ -n "$home" ]; then HOME="$home" "$bin" "$vault" 2> "$log" & pid=$!
  else "$bin" "$vault" 2> "$log" & pid=$!
  fi
  for _ in $(seq 1 300); do grep -q '^launch:' "$log" && break; sleep 0.1; done
  sleep 1
  rss=$(ps -o rss= -p "$pid")
  footprint=$(footprint -p "$pid" --noCategories -f bytes | awk '/phys_footprint:/ {print $2; exit}')
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  echo "$(grep '^launch:' "$log" || echo 'no launch line') | rss ${rss} kB | phys_footprint ${footprint} B"; rm -f "$log"
  sleep 1
done

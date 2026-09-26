#!/usr/bin/env bash
# usage: launch-figure.sh <binary> <vault> <runs>: the launch line, then the process RSS in kB once it is printed
bin=$1; vault=$2; runs=${3:-3}
for i in $(seq 1 "$runs"); do
  log=$(mktemp)
  "$bin" "$vault" 2> "$log" & pid=$!
  for _ in $(seq 1 300); do grep -q '^launch:' "$log" && break; sleep 0.1; done
  sleep 1
  rss=$(ps -o rss= -p "$pid")
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  echo "$(grep '^launch:' "$log" || echo 'no launch line') | rss ${rss} kB"; rm -f "$log"
  sleep 1
done

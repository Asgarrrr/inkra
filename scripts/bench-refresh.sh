#!/usr/bin/env bash
# Runs every published benchmark, warm then cold, into one directory; method in docs/benchmarks.md.
# usage: scripts/bench-refresh.sh [--runs N] [--before REV] [--skip-compare]
# The A/B launch figure compares REV (default 2a30f5c6, before the architecture audit) with HEAD
# on the 50,000-note startup vault. Then bench-compare.sh runs warm, then with --cold.
# Run it from a terminal: sudo asks for the password once, for purge.
set -euo pipefail
cd "$(dirname "$0")/.."

runs=5 before=2a30f5c6 compare=true
while (($#)); do
  case $1 in
    --runs) runs=$2; shift 2 ;;
    --before) before=$2; shift 2 ;;
    --skip-compare) compare=false; shift ;;
    *) echo "usage: $0 [--runs N] [--before REV] [--skip-compare]" >&2; exit 2 ;;
  esac
done

# Another app's CPU and memory skew every figure, so the run refuses to start next to the usual ones.
busy=$(pgrep -il '^(zed|obsidian|cargo|rustc|clippy-driver|cargo-mutants)$' || true)
if [[ -n $busy ]]; then
  printf 'quit these first:\n%s\n' "$busy" >&2
  exit 1
fi

sudo -v
# Keeps the sudo timestamp alive for the whole run; purge runs through `sudo -n`.
while sleep 50; do sudo -n true; kill -0 $$ 2> /dev/null || exit; done 2> /dev/null &
keepalive=$!
caffeinate -dims -w $$ &
trap 'kill $keepalive 2> /dev/null || true' EXIT

before=$(git rev-parse --short "$before")
after=$(git rev-parse --short HEAD)$(git diff --quiet HEAD || echo -dirty)
out=$PWD/target/bench/refresh-$(date +%Y%m%d-%H%M%S)
mkdir -p "$out"
exec > >(tee "$out/run.log") 2>&1

{
  echo "date $(date -u +%Y-%m-%dT%H:%M:%SZ), before $before, after $after, runs $runs"
  echo "$(sysctl -n machdep.cpu.brand_string), $(($(sysctl -n hw.memsize) / 1073741824)) GB, macOS $(sw_vers -productVersion) ($(sw_vers -buildVersion))"
  pmset -g batt | head -1
  uptime
} | tee "$out/meta.txt"

cargo build --release -p inkra-app
after_bin=$out/inkra-$after
cp target/release/inkra-app "$after_bin"

before_bin=$PWD/target/bench/bins/inkra-$before
if [[ ! -x $before_bin ]]; then
  src=$PWD/target/bench/src-$before
  # Registered with git; `git worktree remove` it once the binary is copied.
  [[ -d $src ]] || git worktree add --detach "$src" "$before"
  mkdir -p "$src/vendor"
  [[ -e $src/vendor/zed-reference ]] || ln -s "$PWD/vendor/zed-reference" "$src/vendor/zed-reference"
  (cd "$src" && CARGO_TARGET_DIR=$src/target cargo build --release -p inkra-app)
  mkdir -p "$(dirname "$before_bin")"
  cp "$src/target/release/inkra-app" "$before_bin"
fi

vault=$(cargo run --release -q -p inkra-app --example startup_vault)
echo "vault $vault"

# A fresh HOME per binary; the first launch pays the code-signature check and writes the cache file.
prime() {
  local bin=$1 home=$2 pid
  mkdir -p "$home"
  HOME=$home "$bin" "$vault" 2> /dev/null &
  pid=$!
  for _ in $(seq 1 120); do
    compgen -G "$home/Library/Caches/Inkra/index/*.idx" > /dev/null && break
    sleep 1
  done
  sleep 5
  kill "$pid"
  wait "$pid" 2> /dev/null || true
  ls -l "$home"/Library/Caches/Inkra/index/
}
prime "$before_bin" "$out/home-before"
prime "$after_bin" "$out/home-after"

# Alternating runs, so a drift in the machine's state hits both builds alike.
ab() {
  local mode=$1 flag= build bin line
  [[ $mode == cold ]] && flag=--cold
  for ((i = 1; i <= runs; i++)); do
    for build in before after; do
      bin=$before_bin
      [[ $build == after ]] && bin=$after_bin
      line=$(scripts/launch-figure.sh $flag --home "$out/home-$build" "$bin" "$vault" 1)
      echo "$build $line" | tee -a "$out/ab-$mode.txt"
      if [[ $line == "no launch line"* ]]; then
        echo "the $build build printed no launch line" >&2
        exit 1
      fi
    done
  done
}
ab warm
ab cold

if $compare; then
  for mode in warm cold; do
    flag=
    [[ $mode == cold ]] && flag=--cold
    scripts/bench-compare.sh --runs "$runs" $flag 2>&1 | tee "$out/compare-$mode.log"
    raw=$(sed -n 's/^raw: //p' "$out/compare-$mode.log")
    stamp=${raw##*/results-}
    stamp=${stamp%.jsonl}
    mv "$raw" "$out/results-$mode.jsonl"
    mv "target/bench/summary-$stamp.md" "$out/summary-$mode.md"
    mv "target/bench/logs-$stamp" "$out/logs-$mode"
  done
fi

uptime | tee -a "$out/meta.txt"
echo "done: $out"

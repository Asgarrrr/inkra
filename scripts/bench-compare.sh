#!/usr/bin/env bash
# Compares Inkra with Obsidian on generated vaults; method in docs/benchmarks.md.
# usage: scripts/bench-compare.sh [--runs N] [--sizes 500,5000,50000] [--apps inkra,obsidian] [--cap-first S] [--cap-warm S]
set -euo pipefail
cd "$(dirname "$0")/.."

# A first open may index 50,000 notes from nothing, so it gets the longer busy cap.
runs=5 sizes=500,5000,50000 apps=inkra,obsidian cap_first=600 cap_warm=180
# Between runs, so the disk cache and the CPU settle.
pause=5
while (($#)); do
  case $1 in
    --runs) runs=$2; shift 2 ;;
    --sizes) sizes=$2; shift 2 ;;
    --apps) apps=$2; shift 2 ;;
    --cap-first) cap_first=$2; shift 2 ;;
    --cap-warm) cap_warm=$2; shift 2 ;;
    *) echo "usage: $0 [--runs N] [--sizes 500,5000,50000] [--apps inkra,obsidian] [--cap-first S] [--cap-warm S]" >&2; exit 2 ;;
  esac
done
IFS=, read -ra sizes <<< "$sizes"
IFS=, read -ra apps <<< "$apps"

obsidian_app=/Applications/Obsidian.app
obsidian=$obsidian_app/Contents/MacOS/Obsidian
inkra=$PWD/target/release/inkra-app
out=$PWD/target/bench
profiles=${TMPDIR%/}/inkra-bench-profiles
stamp=$(date +%Y%m%d-%H%M%S)
results=$out/results-$stamp.jsonl
summary=$out/summary-$stamp.md
logs=$out/logs-$stamp

# The benchmark never stops a process it did not start, and a second Obsidian would skew the run.
check_no_obsidian() {
  local pid
  for pid in $(pgrep -f "^$obsidian( |$)" || true); do
    echo "Obsidian is running (pid $pid): quit it first. This script never stops an Obsidian it did not start." >&2
    exit 1
  done
}

uses_obsidian=false
[[ " ${apps[*]} " == *" obsidian "* ]] && uses_obsidian=true
$uses_obsidian && check_no_obsidian

cargo build --release -p inkra-app
mkdir -p "$out" "$profiles" "$logs"
swiftc -O scripts/bench/measure.swift -o "$out/measure"

# A new binary's first exec pays the code-signature check (docs/startup-floor.md): an install cost, so pay it untimed.
if [[ " ${apps[*]} " == *" inkra "* ]]; then
  warmup=$(mktemp -d "$profiles/inkra-warmup.XXXX")
  mkdir "$warmup/home" "$warmup/vault"
  HOME=$warmup/home "$inkra" "$warmup/vault" 2> /dev/null &
  sleep 2
  kill $!
  wait $! || true
fi

jq -nc \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg commit "$(git rev-parse --short HEAD)$(git diff --quiet HEAD || echo -dirty)" \
  --arg cpu "$(sysctl -n machdep.cpu.brand_string)" \
  --argjson memory_gb "$(($(sysctl -n hw.memsize) / 1073741824))" \
  --arg os "macOS $(sw_vers -productVersion) ($(sw_vers -buildVersion))" \
  --arg inkra_version "$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)" \
  --arg obsidian_version "$(defaults read "$obsidian_app/Contents/Info.plist" CFBundleShortVersionString)" \
  --argjson inkra_binary_kb "$(du -sk "$inkra" | cut -f1)" \
  --argjson obsidian_app_kb "$(du -sk "$obsidian_app" | cut -f1)" \
  --argjson runs "$runs" --argjson cap_first_s "$cap_first" --argjson cap_warm_s "$cap_warm" \
  '{meta: $ARGS.named}' > "$results"

# usage: measure <app> <size> <first|warm> <run>; reads $vault, $home and $profile.
measure() {
  local app=$1 size=$2 kind=$3 run=$4 log json cap=$cap_warm
  [[ $kind == first ]] && cap=$cap_first
  log=$logs/$app-$size-$kind-$run.log
  case $app in
    inkra) json=$(HOME=$home "$out/measure" --cap "$cap" -- "$inkra" "$vault" 2> "$log") ;;
    obsidian)
      check_no_obsidian
      json=$("$out/measure" --cap "$cap" -- "$obsidian" "--user-data-dir=$profile" 2> "$log")
      ;;
    *) echo "unknown app $app, expected inkra or obsidian" >&2; exit 2 ;;
  esac
  jq -c --arg app "$app" --argjson size "$size" --arg kind "$kind" --argjson run "$run" \
    --arg launch "$(grep -m1 '^launch:' "$log" || true)" \
    '{app: $app, size: $size, kind: $kind, run: $run} + . + (if $launch == "" then {} else {launch: $launch} end)' \
    <<< "$json" | tee -a "$results"
  sleep "$pause"
}

for size in "${sizes[@]}"; do
  vault=$(cargo run --release -q -p inkra-app --example bench_vault -- "$size")
  # First open: an empty HOME for Inkra; for Obsidian an empty profile and no `.obsidian/` in the vault.
  home=$(mktemp -d "$profiles/inkra-home-$size.XXXX")
  profile=$(mktemp -d "$profiles/obsidian-$size.XXXX")
  if $uses_obsidian && [[ -d $vault/.obsidian ]]; then
    mv "$vault/.obsidian" "$(mktemp -d "$profiles/old-vault-config-$size.XXXX")/"
  fi
  # Automatic updates off: a download mid-benchmark would change the Obsidian under test.
  jq -n --arg id "$(printf '%016x' "$size")" --arg path "$vault" \
    '{vaults: {($id): {path: $path, ts: 1, open: true}}, updateDisabled: true}' > "$profile/obsidian.json"
  for app in "${apps[@]}"; do measure "$app" "$size" first 0; done
  for ((run = 1; run <= runs; run++)); do
    for app in "${apps[@]}"; do measure "$app" "$size" warm "$run"; done
  done
done

{
  jq -r '.meta | "Date \(.date), commit \(.commit). \(.cpu), \(.memory_gb) GB, \(.os).",
    "Inkra \(.inkra_version): \(.inkra_binary_kb / 1024 | round) MB (binary, no .app bundle yet). Obsidian \(.obsidian_version): \(.obsidian_app_kb / 1024 | round) MB (.app).",
    "Warm rows: median of \(.runs) runs. Busy cap \(.cap_first_s) s on first open, \(.cap_warm_s) s warm.", ""' < <(head -1 "$results")
  jq -rs '
    def median: sort | if length == 0 then null elif length % 2 == 1 then .[length / 2 | floor]
      else (.[length / 2 - 1] + .[length / 2]) / 2 end;
    def med(f): map(f | select(type == "number")) | median;
    def cell(scale; digits): if . == null then "-" else . / scale * pow(10; digits) | round / pow(10; digits) | tostring end;
    def lpad(n): tostring | (" " * (n - length)) + .;
    def rpad(n): tostring | . + (" " * (n - length));
    # One bar per row; the largest value in a chart gets 40 characters.
    def chart(title; kind; f; scale; digits; unit): map(select(.kind == kind)) as $rows
      | ([$rows[] | f | numbers] | max) as $top
      | title, ($rows[] | (f) as $v | (if $v == null or $top == 0 then 0 else $v / $top * 40 | round end) as $n
        | "\(.size | lpad(6))  \(.app | rpad(9)) \(if $n < 1 then "▏" else "█" * $n end) \($v | cell(scale; digits)) \(unit)");
    [map(select(.meta == null)) | group_by(.size)[] | group_by(.app)[] | group_by(.kind)[]
      | {size: .[0].size, app: .[0].app, kind: .[0].kind, window: med(.window_ms), busy: med(.busy_ms),
         failed: (map(.busy_ms | strings) | if length == 0 then "" else " (\(length) \(.[0]))" end),
         cpu: med(.cpu_s), memory: med(.footprint_bytes), resident: med(.resident_bytes),
         processes: med(.processes), idle: med(.idle_cpu_pct)}] as $rows
    | "```text",
      ($rows | chart("Window on screen, warm (lower is better)"; "warm"; .window; 1; 0; "ms")), "",
      ($rows | chart("CPU until idle, warm (lower is better)"; "warm"; .cpu; 1; 2; "s")), "",
      ($rows | chart("CPU until idle, first open (lower is better)"; "first"; .cpu; 1; 2; "s")), "",
      ($rows | chart("Memory at idle, warm (lower is better)"; "warm"; .memory; 1048576; 0; "MB")),
      "```", "",
      "| Notes | App | Open | Window on screen (ms) | Busy until idle (s) | CPU until idle (s) | Memory at idle (MB) | Resident (MB) | Processes | Idle CPU (%) |",
      "|---:|---|---|---:|---:|---:|---:|---:|---:|---:|",
      ($rows[] | "| \(.size) | \(.app) | \(.kind) | \(.window | cell(1; 0)) | \(.busy | cell(1000; 1))\(.failed) | \(.cpu | cell(1; 2)) | \(.memory | cell(1048576; 0)) | \(.resident | cell(1048576; 0)) | \(.processes | cell(1; 0)) | \(.idle | cell(1; 2)) |")
  ' "$results"
} | tee "$summary"
echo "raw: $results" >&2

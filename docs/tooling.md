# Tooling — why the scripts and manifests look the way they do

Rationale that used to sit in comments in `scripts/` and the manifests.
Rationale already in a spec is not repeated here: the `shot` feature is in the
off-screen shots spec, the `trash` crate in the create-rename-delete spec.

## `check.sh`

- **`cargo fmt`, without `--all`.** From the root, it formats every workspace
  member and nothing else. `--all` also formats path dependencies; gpui was one
  until it became a git dependency, and a new path dependency would be again.
- **The gpui isolation check.** It must catch gpui below `app` by any route.
  One `cargo tree --workspace --exclude inkra-app` covers every member but
  `app`, so a new crate is checked without an edit to the script:
  - `-e normal,build,dev`: a gpui dev-dependency defeats headless tests as much
    as a normal one.
  - `--target=all`: `cargo tree` otherwise omits dependencies gated on another
    platform.
  - `--all-features`: an optional gpui behind a feature resolves out of the
    tree otherwise.
  - The tree goes into a variable, so a `cargo tree` failure trips `set -e`
    instead of being masked by grep.
  - The match uses a here-string, not `echo … | grep -q`. `grep -q` exits on
    its first match and closes the pipe. `echo` then gets SIGPIPE (141), which
    `pipefail` reports over grep's success, and a real match passes silently.
  - On a match the script prints `cargo tree -i gpui`, the path that brings
    gpui in.

- **The colour literal check.** `rgb(`, `rgba(` or `hsla(` with a numeric first argument fails
  outside `theme/` and `shot/`; `scripts/shot-diff.sh` renders every scene and diffs two renders byte for byte.
- **The inline test module check.** CLAUDE.md puts tests in their own file. The
  check reads `grep -A1` after each `#[cfg(test)]`; a context line prints as
  `file-12-mod tests {`, so `mod` may follow a `-`, not only a space.
- **The crate graph check.** The edges CLAUDE.md's Architecture allows are listed
  in the script, any dependency kind, a crate's dependency on itself excepted
  (the `test-support` pattern below). The `jq` filter keeps dependencies with a
  `path`, so registry and git crates never match. A new crate with no
  workspace dependency passes; its first edge fails until it is listed.
- **The ~400-line warning is in the hook, not here.** Dozens of files are past it
  today, tests included; `check.sh` would print them on every run. The
  PostToolUse hook warns on the file just edited, when the split is cheap.

## Toolchain

`rust-toolchain.toml` pins 1.98.1, the version `check.sh` is green on; it
matches Zed's pin at the gpui rev, which needs it. Bump by re-running `check.sh`.

## Build profiles

`Cargo.toml` has no `[profile]`: neither candidate measured on 2026-11-10 made
`check.sh` faster. Each config got 3 timed runs, cache warm. "no-op" changes
nothing; "touch" touches `crates/text/src/lib.rs` first, an edit cycle.

| config | no-op (s) | touch (s) |
|---|---|---|
| default | 32.6–34.0 | 88.6–91.8 |
| `[profile.dev] debug = "line-tables-only"` | 29.4–32.5 | 101.9–104.4 |
| `[profile.dev.package.proptest] opt-level = 3` | 35.9–39.0* | 82.0–91.7 |

\* A third run, 72.5 s, spiked outside the test step and is left out.

- `line-tables-only` slows the edit cycle by ~12 s, ~9 of them in `cargo test`. The
  release steps, which no dev profile reaches, moved ~1 s, so it is not noise.
- proptest at `opt-level = 3` stays within the spread; the tests' time is probably in
  the code under test, which stays unoptimized.

## Vendored gpui

`crates/app/Cargo.toml` takes gpui and `gpui_platform` from Zed's repository,
pinned by `rev`, never by branch. Cargo fetches it, so a fresh clone builds
with no setup step. `scripts/vendor-gpui.sh` checks out the same rev, read from
`Cargo.lock`, into `vendor/zed-reference`: the source we read is the source we
compile. To bump: edit the three `rev`s, build, re-run the script, re-run
`check.sh`, and re-check every `file:line` citation in `docs/`.

The sparse checkout includes `tooling/` and `extensions/`. They are members of
Zed's workspace, and one missing member fails resolution for the whole build.

## `test-names.sh`

Moving tests between files changes their module path but not their leaf name.
`#[gpui::test]` keeps the outer identifier
(`vendor/zed-reference/crates/gpui_macros/src/test.rs`). A diff of two runs
therefore shows exactly the tests added or dropped. A forgotten `mod` line
drops a whole file's tests and still compiles green.

## Benchmarks

- `criterion` runs without its default `rayon` and `plotters`: no baseline is
  committed and no HTML report is opened. `cargo_bench_support` stays, because
  `cargo bench` refuses a `harness = false` binary without it.
- `inkra-text` and `inkra-markdown` set `bench = false` on their lib. Otherwise `cargo bench` runs
  the lib as a bench harness and passes it criterion flags like `--quick`,
  which it rejects.

## `deny.toml`

- **MPL-2.0** is file-level copyleft. It enters only through two gpui
  dependencies: `cbindgen` (build-time, via `gpui_apple`) and `option-ext`
  (via `dirs` and `zed-font-kit`).
- **`wildcards = "warn"`.** `allow-wildcard-paths` exempts path dependencies of
  private crates only. gpui's manifest is not `publish = false`, so its
  `workspace = true` dependencies would fail `deny` from a file we cannot edit.

## `inkra-vault`'s `test-support`

The feature keeps `MemFs` out of release binaries. `check.sh` runs
`cargo test --workspace` without `--all-features`, so the crate enables it on
itself through a self dev-dependency. `app` uses the same pattern for gpui.
The self dev-dependency is not redundant: in `cargo test --workspace` another
member could enable the feature, but `cargo test -p inkra-vault` alone needs it.

## Lints in tests

`clippy.toml` sets `allow-unwrap-in-tests` and `allow-expect-in-tests`. Clippy
applies them inside `#[test]` functions and `#[cfg(test)]` modules, so test
code carries no `#[allow]`. The deny on `unwrap_used` and `expect_used` stays
for every other line. Integration tests and benches are not `#[cfg(test)]`
modules: a helper there outside a `#[test]` keeps a file-level `#[allow]`.

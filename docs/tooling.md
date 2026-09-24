# Tooling — why the scripts and manifests look the way they do

Rationale that used to sit in comments in `scripts/` and the manifests.
Rationale already in a spec is not repeated here: the `shot` feature is in the
off-screen shots spec, the `trash` crate in the create-rename-delete spec.

## `check.sh`

- **`cargo fmt -p …`, never `--all`.** `--all` formats path dependencies, and
  gpui is one. In `--check` mode it couples the gate to Zed's formatting. Run
  bare, it rewrites the vendored source and breaks its match with the pinned
  commit.
- **The gpui isolation check.** It must catch gpui below `app` by any route:
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
  - A new crate below `app` joins the loop, or it loses the guarantee.

## Toolchain

`rust-toolchain.toml` pins 1.96.1, the version `check.sh` is green on. Zed
pins 1.98.1; do not copy it. Bump by re-running `check.sh`.

## Vendored gpui

`scripts/vendor-gpui.sh` pins Zed by commit, never by branch, so the source we
read is the source we compile. To bump: edit `REV`, re-run the script, re-run
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

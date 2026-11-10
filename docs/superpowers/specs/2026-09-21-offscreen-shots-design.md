# Off-screen shots: seeing what the app renders

Date: 2026-09-21
Status: implemented

## The gap

`check.sh` rasterizes nothing. It runs 269 tests and not one of them draws a
pixel: `TestWindow::draw` renders only when a renderer was installed —
`if let Some(renderer) = &mut state.renderer`
(`gpui/src/platform/test/window.rs:558-566`) — and a `#[gpui::test]` installs
none. A `#[gpui::test]` can assert that a quad was requested; it cannot assert
that anything was drawn, in the right colour, at the right place, with text in
it.

CLAUDE.md already names the consequence and its workaround — *"After touching
the gpui dependency, its features, or `TextElement`, run the app and look at it.
A missing `font-kit` feature silently renders no text while every test still
passes."*

That workaround needs a human at the screen. An agent editing `TextElement` had
no way to check its own work, and the failure mode is silent: the tests stay
green while the window renders nothing. This makes that instruction runnable.

It closes the gap for **development**, not for regression. There are no baseline
images and nothing is compared. CLAUDE.md's rule — *"No snapshot or visual
testing while the UI is still unstable"* — stands, and nothing here revokes it.
Baselines are in Deferred below, with their trigger.

## What gpui already provides

Nothing here is invented. `vendor/zed-reference` ships the whole mechanism:

- `VisualTestAppContext` (`gpui/src/app/visual_test_context.rs`) — a real
  platform for rendering, a `TestDispatcher` for scheduling. Off-screen windows
  at (−10000, −10000) (`:105`), `simulate_keystrokes` (`:225`), `simulate_click`
  (`:314`), `advance_clock` (`:159`), `run_until_parked` (`:152`).
- `Window::render_to_image` (`gpui/src/window.rs:3095`) — renders the last
  frame's scene to a Metal texture and reads the pixels back. Direct texture
  readback, not ScreenCaptureKit: no Screen Recording permission, no visible
  window, no system chrome in the frame.
- Zed drives both from `crates/zed/src/visual_test_runner.rs`, a binary rather
  than a test, gated by `required-features = ["visual-tests"]`.

## Four things building it settled

**The capture is gated on `gpui_platform/test-support`, not
`gpui/test-support`.** `MacWindow::render_to_image`
(`gpui_macos/src/window.rs:2401`) carries `#[cfg(any(test, feature =
"test-support"))]` for *its own* crate. With only gpui's feature the impl
compiles out, the call falls through to `PlatformWindow`'s default
(`gpui/src/platform.rs:1458`), and the runner dies at capture time with
`render_to_image not implemented for this platform`. The feature list has to
name the platform crate. `gpui_platform/Cargo.toml` forwards it:
`test-support = ["gpui/test-support", "gpui_macos/test-support", ...]`.

**`inkra-app` needed a library target.** An example links a package's `[lib]`,
and `inkra-app` had only a `[[bin]]`, so an example could reach nothing in it.

**The example needs `required-features`.** Cargo compiles example targets during
`cargo test` even though it will not run them. Without
`required-features = ["shot"]`, `cargo test --workspace` — a step of `check.sh`
— fails outright on a trampoline naming a module the default feature set gates
away. This is the single sharpest trap in the change, and the reason `check.sh`
needs no edit.

**The frame is in device pixels.** A 1000×640 window captured 2000×1280 on a
2× display.

## Shape

### Crate layout

`inkra-app` is a library with two entry points:

    src/main.rs        fn main() { inkra_app::run() }
    src/lib.rs         module table; actions!(inkra, [Quit]); pub use run
    src/bootstrap.rs   Launch, launch(), run(), and root()
    src/shot/mod.rs    the runner: argv, capture, write          [feature "shot"]
    src/shot/stage.rs  the harness scenes drive                  [feature "shot"]
    src/shot/scenes.rs the catalogue                             [feature "shot"]
    examples/shot.rs   fn main() { inkra_app::shot::run() }

`main.rs` is a trampoline so the runner does not force a single view to become
`pub`. Every view stays `pub(crate)`; the public surface is `run`, `shot::run`,
and `Quit` — the last unavoidably, because `actions!` expands to `pub struct`
(`gpui/src/action.rs:30`). A second entry point is not a reason to widen an API.

One `pub(crate)` item did have to widen its **cfg**, not its visibility:
`sidebar::row_center` was `#[cfg(test)]` and is now
`#[cfg(any(test, feature = "shot"))]`. The runner clicks sidebar rows from a
normal build. Its doc comment already argued that two callers must not each
hard-code the row pitch; the runner is the third.

### What is shared, and what is only mirrored

`bootstrap::root` is the seam. It takes an `Arc<dyn Fs>` and a `Launch` and
returns the root view. The real window calls it with `RealFs`; the runner calls
it with `MemFs`. `root` itself cannot drift.

Everything the *caller* sets up around it can, and an earlier draft of this
design wrongly claimed otherwise. Two things had already drifted when red team
read it:

- `bootstrap::run` registers `cx.on_action(|_: &Quit, cx| cx.quit())`. The first
  runner did not, while still installing `key_bindings()`, which binds `cmd-q`
  to `Quit`. A scene pressing `cmd-q` would have silently no-opped where the
  real app quits — the runner would have shown a state the app never reaches.
- The first runner hard-coded `size(px(1000.), px(640.))` beside
  `bootstrap::WINDOW_SIZE`, so the shot window would have stopped matching the
  real one the day that constant changed.

Both are fixed: `Stage::new` installs the same action handler and the same
bindings, and `Stage::open` is the single door to a window so every scene gets
`WINDOW_SIZE`. The honest claim is narrower than "cannot drift": **the
composition is shared, the surrounding wiring is mirrored deliberately, and
`Stage` is the one place that mirroring lives.**

### Why an example, behind a feature

- **Not a `#[test]`.** `VisualTestAppContext` needs a real `MacPlatform`, whose
  constructor asserts the main thread:
  `main_thread_marker().expect("Mac platform not created on main thread")`
  (`gpui_macos/src/platform.rs:217`). `cargo test` runs tests on worker threads.
  (`crates/zed/src/zed/visual_tests.rs:11`, in the **zed** crate, says the same
  in prose; the assertion is the load-bearing evidence.)
- **Not a `[[bin]]`,** though the reason is narrower than it first appears. A
  binary cannot use dev-dependencies, but the `shot` feature turns the runner's
  dependencies into ordinary ones, so that objection does not apply. What
  remains: an example is not a shipped artifact. `cargo build --release`
  produces no example, so the tool cannot end up in a release directory or be
  installed by `cargo install`.
- **Behind `shot`,** so `cargo build --release` never compiles the runner and
  never links `test-support`. Clippy in `check.sh` runs
  `--all-targets --all-features`, so the module is still type-checked and linted
  under `-D warnings` — verified: it rejected three `{path:?}` formats and a
  `single_match_else` while this was being written. It cannot rot unnoticed.

### The catalogue

A scene is a named function that opens a window and leaves it in the state worth
looking at. Rust, compiled in, not a script language: a scene assembles a
`MemFs`, a `Launch` and keystrokes with the same vocabulary the existing test
helpers use, and a mini-language would be a permanent component whose
expressiveness always lags `EditorState`.

```rust
pub(crate) struct Scene {
    name: &'static str,   // the CLI name, and the PNG's stem
    shows: &'static str,  // printed by --list
    open: fn(&mut Stage) -> WindowHandle<AppShell>,
}
```

`Stage` offers exactly what a scene needs: `open_nothing` / `open_note` /
`open_vault`, then `focus`, `keys`, `click`, `capture`. A scene needing more
grows `Stage`, in the same change, which keeps the runner's reach readable in
one file.

| Scene | What it shows |
|---|---|
| `empty-no-vault` | Launch with no argument: the empty state, and no editor at all. Renamed from `editor-empty` on 2026-09-21, when the buffer it showed stopped existing |
| `editor-text` | Per-line layout, a blank line, real font metrics, unfocused |
| `editor-caret` | The caret mid-document, in a focused window |
| `editor-selection` | A selection over four lines: both ends, and a blank line's stub |
| `sidebar-vault` | The tree: directories first, indents, a nested note |
| `sidebar-dirty` | The dirty dot on a note edited but not written |

Each `Stage` is built per scene, so no scene inherits another's clipboard, focus
or open windows.

### Command line

    cargo run -p inkra-app --features shot --example shot
    cargo run -p inkra-app --features shot --example shot -- editor-selection
    cargo run -p inkra-app --features shot --example shot -- --list

No argument shoots every scene. A name shoots one. An unknown name prints the
catalogue and exits 2 — a typo must not read as a pass. Output goes to
`target/shots/<name>.png`, already gitignored, and each path is printed.

### Focus

`open_offscreen_window` passes `focus: false`
(`gpui/src/app/visual_test_context.rs:113`), and an unfocused editor paints no
caret. `Stage::focus` is therefore explicit rather than automatic: the unfocused
frame is a state worth shooting, and `editor-text` shoots it.

## Errors

The runner panics on every failure, deliberately. It is run by hand: a panic
with a backtrace names the failing scene and the line, where a propagated
`Result` would print a message nobody is positioned to act on. The crate denies
`unwrap_used` and `expect_used`, so each failure point uses
`unwrap_or_else(|error| panic!("<what was being done>: {error}"))`.

One correction that clippy forced: a path in a panic message takes
`{}` with `.display()`, never `{path:?}` — `clippy::unnecessary_debug_formatting`
is pedantic-level and `check.sh` promotes it to an error.

The one non-panic path is an unknown scene name, which is user error rather than
a defect: catalogue, exit 2.

## Testing

The runner is a testing tool, which does not exempt it from being wrong. What
guards it:

- `check.sh`'s clippy pass compiles it under `--all-targets --all-features`, so
  it type-checks against every view it touches. A view whose constructor changes
  breaks the build there, not silently at the next hand-run.
- `bootstrap::root` is exercised by the real binary on every launch, so the
  shared seam cannot rot while only the runner uses it.

`check.sh` deliberately does **not** run the runner. Rendering needs a window
server and a GPU, the output is display-dependent, and there is nothing to
assert against without baselines. Running it in the gate would buy a flaky check
and no signal.

## Limits

- **macOS only.** `gpui_macos` and `gpui_windows` implement
  `render_to_image`; `gpui_linux` does not. The `shot` feature builds on Linux
  and would panic at capture time with the `PlatformWindow` default's message.
  Consistent with the project's macOS-first posture, but not gated — there is no
  Linux target to gate for yet.
- **Not reproducible across machines.** Display scale (2× here, 1× elsewhere)
  and system fonts both vary. `font-kit` resolves fonts installed on the
  machine, and their version, hinting and antialiasing differ across OS
  versions. This costs nothing today, because the only consumer is a reader.

## Deferred

| Item | Trigger to revisit |
|---|---|
| Baseline images and a diff | When the UI stops moving — the point at which CLAUDE.md's "no snapshot testing" rule is worth revoking. Needs a tolerance, a per-pixel threshold, a display-independent capture size, **and a pinned font**, per Limits above. |
| Running shots in `check.sh` | Follows baselines; pointless before them. |
| Driving the real binary (system keyboard, AX output) | When something must be proven that only the real `MacPlatform` does — IME, menus, the accessibility tree. `scripts/ax-text-probe.swift` is the existing precedent and stays the tool for it. |
| A scene script language | When writing a scene in Rust is demonstrably the bottleneck. It has not been tried yet. |
| Forcing a 1× capture | When a shot has to be compared across machines. |

## What this does not change

- No crate boundary moves. `inkra-text`, `inkra-editor` and `inkra-vault` keep
  their zero-gpui guarantee, and `check.sh`'s isolation loop is untouched.
- No view becomes `pub`. One `#[cfg(test)]` widens to include the feature.
- `check.sh` is unedited, and stays green: 269 tests, `cargo build --release`
  clean, no example compiled into it.

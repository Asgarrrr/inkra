# gpui-test — architecture design

Date: 2026-09-19
Status: approved

## Purpose

`gpui-test` is a prototype text editor built on GPUI (Zed's Rust UI
framework). It exists to evaluate whether a full-Rust stack (GPUI) is a
viable replacement for Inkra's current Tauri v2 + React stack. Scope for
this prototype is deliberately small: a single text-editing view, no
sidebar, no file tree, no tabs.

This document was produced through a design review that included an
adversarial pass: one agent tried to break the proposed architecture
(grounded in Zed's own source), one agent looked for concrete
improvements. Both passes are folded into the design below; see
"Rejected and deferred options" for what they ruled out.

## Architecture

Cargo workspace, two crates:

```
gpui-test/
  Cargo.toml       # workspace root (virtual manifest)
  crates/
    core/           # gpui-test-core
    app/            # gpui-test-app (binary)
```

Dependency direction: `core → app`, one-directional. `core` has zero
dependency on `gpui` or any GPUI type. This is verified in CI (see
Verification), not just by convention.

`core` and `app` were `core`/`ui`/`app` in the first draft. The `ui`/`app`
split was dropped: for a single-window, single-binary prototype it adds no
tested value (the `app` crate would hold `main()` plus a few lines of
assembly). The boundary that actually pays off — `core` without GPUI,
giving fast headless tests and short compile times for the buffer logic —
is preserved. Re-splitting `app` into `ui` + `app` is a ten-minute change
if a second binary or a dedicated UI test suite ever justifies it.

## Components

### `core` (gpui-free)

Owns: text buffer, insert/delete operations, offsets (byte and char
newtypes), undo history.

Does **not** own: cursor movement, selection, line wrapping, or anything
else that depends on display layout (glyph widths, wrapped-line
positions). That information only exists once GPUI has laid out the
text — putting movement/selection logic in `core` would force either
layout data to leak backward into `core`, or a layout abstraction to be
invented inside `core` purely to satisfy the boundary. Neither is
justified at this scope. This mirrors Zed's own split: `text`/`rope`
(gpui-free) vs. `editor` (gpui-dependent, owns movement and selection).

Public surface: buffer construction, edit operations, offset newtypes,
undo/redo. No `gpui` type appears in any public signature.

### `app` (gpui-dependent, binary)

Owns: the GPUI `Entity<T>` wrapper around `core`'s buffer, cursor and
selection state, the `EntityInputHandler` implementation GPUI requires
for text input (GPUI ships no built-in text input widget), and `main()`.

GPUI's input protocol works in UTF-16 offsets. `core`'s offsets are byte
and/or char based. The conversion between the two lives entirely in
`app` — `core` is never aware UTF-16 offsets exist. This conversion layer
is expected to be the bulk of the prototype's implementation effort, not
the crate structure.

## Data flow

User input (keystroke, IME event) → GPUI delivers it to `app`'s
`EntityInputHandler` impl → `app` converts UTF-16 offsets to `core`'s
offset space → `app` calls the relevant `core` edit operation → `core`
mutates the buffer and returns the new state → `app`'s `Entity` wrapper
calls `cx.notify()` → GPUI re-renders the affected view, using `app`-side
layout to resolve any movement/selection changes.

## Error handling

Typed errors (`thiserror`) are reserved for genuinely fallible
boundaries — file I/O, once that exists. In-memory buffer operations
(insert/delete/offset lookups) are not wrapped in `Result`: an invalid
offset passed to `core` is a caller bug, not a recoverable case, and is
handled with `debug_assert!`/panic rather than propagated as an error
type. Blanket `thiserror` on every public function was the first draft's
approach; it was dropped because it adds `Result` noise to call sites
that can't meaningfully fail, without catching anything real.

`clippy::unwrap_used` and `clippy::expect_used` are `deny` at the
workspace level regardless — panicking is fine for genuine invariant
violations, silently swallowing an error is not. Test modules get a
local `#[cfg_attr(test, allow(clippy::unwrap_used))]` override.

## Conventions

- Newtypes for offsets: `core` distinguishes byte and char offsets by
  type. `app` adds the UTF-16 conversion layer on top — three offset
  spaces total, kept apart by the type system rather than by convention,
  because mixing them is the most common bug class in this kind of code.
- `[workspace.lints]` in the root `Cargo.toml`, inherited by every crate
  via `[lints] workspace = true`:
  - `unsafe_code = "deny"`
  - `unreachable_pub = "warn"` — forces every crate-boundary-crossing
    `pub` to be a conscious decision.
  - `clippy::all`, `clippy::pedantic` = `"warn"` (not `deny` — pedantic
    is intentionally noisy; real enforcement happens via `-D warnings`
    in CI, not in the checked-in lint level, so local `cargo clippy`
    stays usable during development).
  - `clippy::unwrap_used`, `clippy::expect_used` = `"deny"`.

## Testing

- `core`: plain `#[test]` unit tests, plus `proptest` for edit
  sequences — generate random sequences of insert/delete operations,
  apply them to the real buffer and to a trivial reference model
  (`Vec<char>` or `String`), assert equality and offset-newtype
  invariants after each step. Commit `.proptest-regressions` once the
  first proptest exists, so a discovered failure stays a regression test
  forever.
- `app`: `gpui::TestAppContext` / `#[gpui::test]`, which run without a
  real window (deterministic simulated executor) — usable in headless
  CI. Snapshot/visual testing (`VisualTestAppContext`, real GPU
  rendering) is deferred; it requires a CI runner with real rendering
  and isn't justified while the UI is a single unstable prototype view.

## Verification

CI, in order (fast/local checks first):

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo tree -p gpui-test-core -i gpui && exit 1 || true   # fails the build if core ever gains a gpui dependency
cargo test --workspace
cargo build --workspace --release
```

The `cargo tree` check is the mechanical enforcement of the `core → app`
one-directional rule — it fails CI the moment `core` picks up a
transitive or direct dependency on `gpui`, rather than relying on
code-review discipline to catch it.

## Rejected and deferred options

Ruled out for now, with the reasoning that would need to change before
revisiting:

- **3 crates (`core`/`ui`/`app`)** — deferred until a second `ui`
  consumer or a dedicated UI test suite exists (see Architecture).
- **Blanket `thiserror` on every crate boundary** — dropped; buffer ops
  are infallible, see Error handling.
- **Shared types/errors crate** — there is only one public boundary
  (`core → app`); `thiserror` inside `core` covers it. Revisit only if a
  third crate needs the same error types without depending on `core`
  directly.
- **`test-utils` crate** — no two crates share fixtures yet. Revisit once
  they do.
- **`cargo-deny` (bans/licenses/advisories)** — the `cargo tree` check
  already covers the one invariant that matters today. Adopt full
  `cargo-deny` once the project passes ~3 crates or takes on
  security-sensitive third-party dependencies.
- **`cargo machete`** — adopt once the third-party dependency list grows
  past a handful of entries.
- **Splitting `core` into Zed-style sub-crates** (`rope`, `collections`,
  etc.)** — premature; revisit only once `core` itself grows past a
  handful of modules (rule of three: duplicate twice before extracting).

## Open questions

None blocking. The `EntityInputHandler`/UTF-16 conversion layer (see
`app` component) is flagged as the likely largest source of unknowns in
implementation, but it does not change this architecture — it is
scoped work inside `app`.

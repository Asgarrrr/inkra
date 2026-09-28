# The launch floor

Profiled on 2026-09-28 for the roadmap's "window start-up floor" item. The
question: where do the ~150–180 ms before the first frame go, against the
100 ms budget, and what can Inkra cut? The answer: almost nothing. macOS spends
that time before any AppKit app shows a window, and Inkra launches within a few
ms of a bare AppKit window. The `launch:` line's `first frame` read ~27 ms
late; it was fixed on 2026-09-29 (below).

Machine: MacBook Pro, Apple M5 Pro, macOS 26.5. Release build, warm cache,
one-note vault. Every figure is a median of 8 to 14 runs; run-to-run noise is
±10 ms and batch-to-batch noise ±20 ms, so compare only interleaved runs.

## The oracle

`scripts/window-onscreen.swift` spawns a program and polls the WindowServer
until the program's window is on screen. That time, from spawn, is the launch
as the user sees it. It includes exec and dyld, which `Timeline` does not see
(`run()` starts ~10–14 ms after spawn).

`scripts/appkit-floor.swift` is the floor: one `NSWindow` of Inkra's size, one
`CAMetalLayer`, one cleared frame, nothing else.

| Program | Window on screen, from spawn |
|---|---|
| `appkit-floor` | 156–167 ms |
| Inkra | 166–168 ms |

Inkra is 1–11 ms above the floor across four interleaved batches. 100 ms from
spawn is below what AppKit itself allows on this machine.

## Where the floor goes

`appkit-floor`'s own steps, from spawn:

| Step | ms |
|---|---|
| `main` reached (dyld) | 14 |
| `NSApplication.shared` returns | 60 |
| `applicationDidFinishLaunching` | 90 |
| `NSWindow` created | 118 |
| ordered front and activated | 130 |
| first Metal frame committed | 152 |
| `presentedHandler` fires | 158 |

About half of it is off-CPU: XPC to the WindowServer, LaunchServices and the
text input server. A `.app` bundle changes nothing (tested for both programs).

## What gpui adds

A samply profile of Inkra's main thread shows gpui's steps before the first
draw. The first three touch a system service that some later AppKit call
would start anyway, so the cost moves rather than disappears:

| gpui step | Wall | Skipped (env-gated patch, interleaved) |
|---|---|---|
| `MacKeyboardLayout::new` (TIS, HIToolbox) | 25–28 ms | no change |
| `ensure_system_power_observers` (`NSWorkspace`) | 18–20 ms | no change |
| `tab_bar_visible` and `tabbed_windows` | 8.5–9.6 ms | −4 ms |
| all three | | −6 ms |
| `MacWindow::open` | 44 ms | matches AppKit's window creation and ordering |

After the first draw, AppKit shows the window: ~23 ms on the main thread with
no gpui or Inkra frame on the stack (ViewBridge XPC, a `dlopen`). Only then
does gpui create the display link.

## What Inkra adds

Building the root views takes under 1 ms. The first frame's draw, from its
next-frame callback to the Metal commit, takes ~4.5 ms. The vault costs the
first frame nothing (open-first spec, O6).

## Why `first frame` reads late

`bootstrap::run` marks `FirstFrame` in a next-frame callback nested in another,
so it fires at the second frame. gpui draws the first frame from
`display_layer` (`gpui_macos/src/window.rs`, `display_layer`), presented with
the window's Core Animation transaction, and only then starts the display
link. The second callback waits for that link's first tick.

| Probe, from spawn | ms |
|---|---|
| first next-frame callback | 138–145 |
| first Metal commit (`MetalRenderer::draw`) | 143–149 |
| `FirstFrame` mark | 170–178 |

A drawable's `presentedHandler` fires at the commit under a transaction
present, so it cannot tell when pixels reach the glass. The WindowServer
oracle can.

### The fix, 2026-09-29

`FirstFrame` is now marked in the first next-frame callback: the start of the
first draw, ~4.5 ms before its commit. gpui runs next-frame callbacks and then
draws and presents in the same turn (`gpui/src/window.rs`, `on_request_frame`),
so no hook in Inkra's code runs between the commit and AppKit's work that
shows the window.

A task spawned from the callback was tried first. It runs after the commit,
but it sometimes queues behind the ~23 ms of AppKit work that shows the
window. It read 132–137 ms in half the runs and 158–162 ms in the others.

Interleaved, 8 runs each, one-note vault, from `run()` start:

| Mark | `first frame` | Median |
|---|---|---|
| second frame (before) | 161–186 ms | ~167 ms |
| first draw's start (after) | 124–147 ms | ~139 ms |

## Levers

| Lever | Gain | Cost |
|---|---|---|
| Patch gpui: skip the tab-bar queries on a window with no tabbing identifier | ~4 ms, inside the noise | A patch on the vendored gpui, rebased on every bump |
| Defer the keyboard layout and power observers | none measured | same |
| Reopen a window from the running process: closing the window already leaves the process alive, but nothing handles a Dock click (`App::on_reopen`) | Probably most of the floor for every launch after the first; not measured | A product choice: the app stays resident, and `⌘Q` still quits it |

No change inside Inkra's code moves the launch. The mark on the second frame
was the one defect, and it is fixed. For the launch as the user sees it, use
the WindowServer oracle.

Not measured: a cold launch after a reboot. The first run after each rebuild
took 430–600 ms, but that run pays for the new binary's code-signature check
too, so it is not a cold-launch figure.

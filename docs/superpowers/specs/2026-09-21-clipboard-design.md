# Clipboard: copy, cut, paste

Status: implemented (2026-09-21), after red/blue review. Every claim citing a
file:line was verified against the vendored source, and the section "What the
review changed" records where designing it and checking it disagreed.

`⌘C`, `⌘X` and `⌘V` do not exist. `crates/app/src/keymap.rs` binds twenty
actions and none of them is a clipboard action. An editor that cannot copy a
line is not usable for writing, whatever else it grows, so this lands before
the link features that make the application a note application.

Scope: one selection, text only. Everything else is in Deferred with a trigger.

## What gpui gives us

Two primitives, and nothing built on them:

- `App::write_to_clipboard(ClipboardItem)` and
  `App::read_from_clipboard() -> Option<ClipboardItem>`
  (`gpui/src/app.rs:1546`, `:1518`). `read_from_clipboard_async` (`:1529`)
  exists for platforms where the read is permission-gated; macOS is not one.
- `ClipboardItem { entries: Vec<ClipboardEntry> }`, where an entry is a
  `String`, an `Image` or `ExternalPaths` (`gpui/src/platform.rs:2652`,
  `:2693`).

There is no default binding and no `NSTextView` underneath. The macOS
`cut:`/`copy:`/`paste:` selectors are registered
(`gpui_macos/src/platform.rs:110-122`) but every one of them routes to
`handle_menu_item`, which dispatches the gpui action; the comment above them
says they exist so the OS save panels show the right key equivalents. The menu
mapping is `:364-374`. So a clipboard action is an ordinary application action,
declared and bound like `Undo`.

## What Zed does, and what we take

Zed keeps clipboard in its own module, `crates/editor/src/clipboard.rs`, 763
lines, three entry points: `copy` → `do_copy` (`:541`, `:581`), `cut` →
`cut_common` (`:418`, `:360`), `paste` → `paste_item` → `do_paste` (`:252`,
`:258`, `:60`).

**Taken: an empty selection means the whole line.** `do_copy` computes
`is_entire_line = selection.is_empty() || line_mode` (`:593`) and widens the
range to `[line_start, next_line_start)`; on a last line with no trailing
newline it stops at the line end and appends one (`:659-662`). `cut` passes
`cut_no_selection_line: true` (`:422`). The paste half is the other side of the
same rule: an entire-line item pasted into an empty selection is inserted
*before* the caret's line rather than at the caret (`:125-132`). The three only
make sense together. CodeMirror 6, which Obsidian runs on, shares both halves —
one `copiedRange(view.state)` feeds copy and cut, and `lastLinewiseCopy` gates
the paste — and so does VS Code, under `editor.emptySelectionClipboard`, on by
default. This is the convention, not a Zed preference.

**Taken: the transaction discipline.** `do_paste` calls
`finalize_last_transaction` before the edit and wraps the edit in `transact`
(`:72`, `:76`); `cut_common` deletes inside `transact` (`:409-414`). This is
CLAUDE.md's existing rule, confirmed against the reference rather than
asserted. What does *not* transfer is the mechanism — see the first review
correction below.

**Rejected: the selection metadata.** Zed serializes a
`Vec<ClipboardSelection>` as JSON beside the text, and on paste hands each
cursor its own slice when the counts match (`:85-122`). That is multi-cursor
machinery. `Selection` is one range by construction, so none of it applies.

**Rejected outright:** image paste and the markdown attachment flow
(`:268-322`), `CopyAndTrim` (`:532`), the kill ring (`:426`, `:514`), and
`diff_clipboard_with_selection` (`:545`).

## Where the metadata actually lives

We still need one bit across the round trip — whether the copy was a whole
line. `ClipboardItem::new_string_with_metadata(text, String)`
(`gpui/src/platform.rs:2711`) carries an opaque string, so no `serde`
dependency is needed; neither `app` nor `editor` has one today.

Inferring the flag instead — "the text ends in `\n` and the selection is
empty" — was considered and rejected. It misfires on ordinary pastes: a whole
file's contents, a shell command's output, a paragraph from a web page all end
in a newline and would be inserted above the caret's line instead of at it. The
signal is *how the text was selected when it was copied*, which no inspection
of the text can recover. All three reference implementations store it
explicitly: Zed in `ClipboardSelection::is_entire_line`, VS Code in its
internal clipboard metadata, CodeMirror 6 in a module-level `lastLinewiseCopy`
guarded by an exact text match.

The macOS pasteboard stores it under a custom type literally named
`"zed-metadata"` (`gpui_macos/src/pasteboard.rs:48`), which is not
configurable. Two consequences, recorded rather than discovered:

- Reading is guarded by a hash of the text (`:126-138`): the metadata is
  returned only when the text on the pasteboard still hashes to what was
  written with it. Another application overwriting the text drops our flag
  instead of misapplying it. That is the behaviour we want, for free.
- We write under Zed's type name, and Zed writes JSON under it. So our reader
  must treat the metadata as an exact match against one token and nothing else:
  any other value, including Zed's `[{"len":…}]`, means "not a whole line". A
  Zed line copy pasted into Inkra therefore pastes as an ordinary string. That
  is acceptable; guessing at a foreign format is not.

`TestPlatform` stores the whole `ClipboardItem` in a mutex
(`gpui/src/platform/test/platform.rs:671-676`), with no hash guard, and
`TestAppContext::{write,read}_from_clipboard` exposes it
(`gpui/src/app/test_context.rs:376`, `:382`). So tests round-trip the metadata
faithfully but cannot exercise the hash guard. Noted as a known blind spot, not
worked around.

## Design

### Layering

What range a copy takes and how a paste lands needs no pixels, no clock and no
platform, so it is `inkra-editor`, beside `EditorState::insert`. The
`ClipboardItem` is gpui, so it is `app`. The split is the same one every other
action already uses, and it has room for the feature that comes next: pasting a
URL over a selection to produce a markdown link is a pure text operation in Zed
too (`clipboard.rs:713-731`, `:756-763`), and `EditorState::paste` already
receives everything it needs to do it — the selection, the buffer and the
pasted text.

### `inkra-editor`

```rust
/// What a copy took, and how a paste of it should land.
pub struct Copied {
    pub text: String,
    /// The copy was a whole line. `text` ends with `\n`, and pasting it at a
    /// bare caret inserts a line above rather than splitting the current one.
    pub whole_line: bool,
}

impl EditorState {
    /// Reads only. An empty selection takes the caret's whole line.
    #[must_use]
    pub fn copy(&self, buffer: &Buffer) -> Copied;

    /// `copy`, then removes what it took, in one undo step that merges with
    /// neither neighbour.
    pub fn cut(&mut self, buffer: &mut Buffer, now: Instant) -> Copied;

    /// Replaces the selection, or — for a `whole_line` item at a bare caret —
    /// inserts the line above the caret's line.
    pub fn paste(&mut self, buffer: &mut Buffer, now: Instant, copied: &Copied);
}
```

Built from what exists. The line arithmetic comes from
`Buffer::line_for_offset`, `line_range` and `line_count` — note that
`line_range` excludes the terminating newline by design
(`text/src/buffer.rs:357-361`), so widening to include it is this code's own
step and must not be re-derived by scanning the text again. `paste`'s ordinary
branch — anything not a whole-line item at a bare caret — *is*
`EditorState::insert` (`editor/src/state.rs:106-109`), whose doc comment
already says "what pasting will do"; it calls it rather than reimplementing it.
`cut` reuses `remove`'s empty-range guard (`editor/src/state.rs:132`).

**The one postcondition everything else rests on:** a whole-line `Copied.text`
always ends in exactly one `\n`, synthesized when the source line has none.
This holds for `cut` as well as `copy`, which is where we part company with
Zed — see the second review correction.

**The caret after a whole-line paste** stays in its original column, on the
line that was pushed down: `caret_before + text.len()`, not the `line_start +
text.len()` that `EditorState::edit` would leave behind. `paste` therefore
places the caret itself after the edit rather than accepting `edit`'s default.

### `app`

Three actions in `crates/app/src/editor/actions.rs` — `Copy`, `Cut`, `Paste` —
and three bindings in `keymap.rs`: `cmd-c`, `cmd-x`, `cmd-v`, context `None`,
matching every other editor binding. Context `None` makes the *binding* match
anywhere; the handler still only runs when the editor is in the dispatch path,
because `on_action` is registered on the editor's own div
(`app/src/editor/mod.rs:257-274`).

Gates:

- `Cut` and `Paste` go through `edit_buffer`. They mutate text.
- `Copy` goes through **no gate**, and is the first action that does not. It
  mutates nothing and owes no repaint, so `update_state` and `move_cursor` are
  both wrong: either would notify for a frame identical to the last one, and
  `edit_buffer`'s `as_mut` notifies unconditionally
  (`gpui/src/app.rs:3280-3286`), which would schedule an autosave for a
  keystroke that changed no text. It reads with `self.buffer.read(cx)` and
  nothing else. CLAUDE.md's gates paragraph and
  `app/src/editor/actions.rs`'s module comment both claim every action goes
  through a gate; both are corrected in this change, not after it.

### Four edges that are defects if missed

**A paste or a cut must be sealed on both sides.**
`Buffer::finalize_transaction` sets `grouping_blocked`
(`text/src/buffer.rs:151-153`), and `Buffer::close` clears it again the moment
it pushes a transaction (`:190`). One call before the edit therefore stops the
paste merging *backwards* into a typing run and does nothing about the
keystroke that lands 200 ms *after* it, which merges forwards into the paste.
Zed does not hit this because its edit goes through the ungrouped
`Buffer::transact`, whose transaction records `last_edit_at: None` and can
never be merged into; every mutation we have takes `now` and goes through
`transact_at`. So both `cut` and `paste` call `finalize_transaction` before the
edit *and* again after it.

**An empty clipboard must change nothing — and `Some("")` is empty.**
`item.text()` returns `Option<String>` (`gpui/src/platform.rs:2738`), but a
pasteboard holding a present-but-empty string entry yields `Some("")`, not
`None` (`gpui_macos/src/pasteboard.rs:114-141`). Either one means return
before touching the buffer. Not merely "insert an empty string": `Buffer::close`
returns early on an empty edit list (`text/src/buffer.rs:174`), so
`grouping_blocked` is never cleared, and an ineffective paste in the middle of
a typing run would split that run into two undo steps. The guard is
`item.text().filter(|text| !text.is_empty())`, evaluated *before* finalizing.

**A cut that removes nothing must record nothing.** `⌘X` on an empty last line
copies the synthesized `"\n"` but its deletion range is empty.
`EditorState::remove` already guards this way (`editor/src/state.rs:132`); `cut`
owes the same guard, and must also skip both `finalize_transaction` calls, for
the reason above.

**Pasted text may carry CRLF.** `read_from_clipboard` returns the pasteboard
string verbatim (`gpui_macos/src/platform.rs:1159-1162`). `Buffer` assumes LF —
that is why `Fs::load` normalizes — so paste is a second door into the buffer
that bypasses the first door's guard. Copying from a Windows-authored document
would put lone `\r` bytes in the text.

Decision: make `inkra_vault::normalize_line_endings` public
(`vault/src/fs.rs:121`, `pub(crate)` today) and call it in `app` before the
text reaches `EditorState::paste`. `app` already depends on `vault`, so this
adds no edge to the crate graph and `scripts/check.sh`'s isolation loop
(`:14-39`) is unaffected — it tests for a gpui edge, not for edges among
`text`, `editor` and `vault`. The two alternatives were weighed and rejected:
a copy in `inkra-text` cannot be shared, because `inkra-vault` must not depend
on `inkra-text` (CLAUDE.md), and a private copy in `app` would mean the same
ten lines and the same four tests (`vault/src/fs.rs:178-195`) maintained twice —
and the next question about them, U+2028 and U+2029, would then have to be
answered twice and could drift. The smell is real and stated rather than
hidden: a function named for the filesystem layer serves a clipboard paste. It
belongs to *text arriving from outside*, and both doors sit inside `app`'s
dependency cone.

## What the review changed

Four corrections, each a design decision rather than a coding mistake.

**1. One `finalize_transaction` was not enough.** The first draft copied Zed's
prose — finalize, then edit — without checking that Zed's edit is *ungrouped*
and ours cannot be. Traced through `Buffer::close` (`text/src/buffer.rs:170-195`)
the draft left a paste absorbing the next keystroke into its own undo step, and
said nothing at all about cut, which Zed isolates identically. Both sides, both
operations, and a test for each.

**2. The round-trip claim was false, and Zed solves it the other way.** The
draft asserted that `cut` followed by `paste` restores the text, and separately
that a whole-line copy always ends in `\n`. Those two cannot both hold. Cut the
only line of the buffer `"a"`: the copy synthesizes `"a\n"`, the deletion
removes the one byte that was there, and pasting gives `"a\n"` — a trailing
newline the buffer never had.

Reading the source settles what Zed does: `cut_common` does **not** synthesize
a trailing newline (`clipboard.rs:374-406`; the `add_trailing_newline` branch
exists only in `do_copy`, `:659-662`), and `do_paste` inserts the clipboard
text verbatim at `line_start` (`:132`). Zed's cut therefore puts exactly the
removed bytes on the clipboard, and its round trip is exact — at the cost of
the opposite defect: cut the last line of `"x\ny"` and paste it on line 0 and
you get `"yx\n"`, two lines joined.

We take the other trade. `cut` synthesizes the newline like `copy` does, so
every whole-line item is a complete line wherever it lands — which is what
cutting a line is *for*. The price is stated, tested and bounded: cut then
paste on a last line that had no terminator gives the line its terminator back.
For a markdown note a file ending in a newline is the convention anyway, and
two lines silently joined is not.

The proptest is restated to match, as a conditional property in the style
`vault/tests/conflict_proptest.rs` already uses: *cut then paste at the
resulting caret restores the original text exactly whenever the cut range ended
at a newline or the selection was non-empty; otherwise it differs by exactly
one trailing newline, and never by anything else.*

**3. The caret landed in the wrong column after a whole-line paste.** The draft
let `EditorState::edit` place it, which is `line_start + text.len()` — column 0
of the line that was pushed down. Every reference implementation keeps the
caret's original column instead: Zed's selections are anchors, so an insertion
entirely before them shifts them by its length (`clipboard.rs:168-171`); VS Code
uses `ReplaceCommandThatPreservesSelection`; CodeMirror's changelog records
fixing exactly this as a bug in 0.17.9. `paste` places the caret itself.

**4. The IME hazard is real, pre-existing, and not this change's to fix.** Cut
or paste during an input-method composition clears `marked_range` through
`EditorState::edit` (`editor/src/state.rs:101`) while the OS still believes it
is composing; the next candidate lands wherever `target_range` falls back to
(`app/src/editor/input.rs:38-41`). This is exactly what `⌘Z` does today —
`settle_on` clears the same field (`editor/src/state.rs:221-224`) — so
clipboard neither introduces the hazard nor worsens it. Deferred with a
trigger rather than fixed here, because fixing it properly means telling the
platform to abandon its session, and there is no headless test that can prove
we did.

## Tests

`inkra-editor`, plain `#[test]` in the existing `mod tests`, reusing its `at`
and `editor` helpers (`editor/src/state.rs:232-240`):

1. A copy with a selection takes exactly that range, `whole_line == false`.
2. A copy with an empty selection takes the caret's line and ends with `\n`.
3. The same on a last line with no trailing newline — still ends with `\n`.
4. A cut removes what it put on the clipboard, in one undo step.
5. A cut with an empty selection removes the line *and* its newline.
6. A cut with nothing to remove records no undo step and does not seal the
   surrounding transaction.
7. Pasting a non-line item replaces the selection.
8. Pasting a line item at a bare caret inserts above the caret's line **and
   leaves the caret in its original column**, on the pushed-down line.
9. Pasting a line item over a selection replaces the selection.
10. A paste does not merge into the typing before it or the typing after it.
11. Neither does a cut — the same assertion, for the operation the draft forgot.
12. Cut the only line of `"a"`, paste it back: the text is `"a\n"`. The stated
    price of correction 2, pinned so it cannot change silently.

`inkra-editor`, a third property in the existing `tests/editor_proptest.rs`,
asserting the conditional round-trip from correction 2.

It asserts the caret too — the neighbouring `typing_over_a_selection_replaces_exactly_it`
sets that bar — but **only for a non-empty selection**, and that exception was
found by writing it. A line cut leaves the caret at the line's start; the paste
then inserts the line *at* that start, pushing the caret to the beginning of
the line below the restored one. So the text round-trips and the caret does
not. Zed lands in the same place for the same reason, its anchors shifting by
the inserted length. Asserting it unconditionally would have meant asserting
something false.

`app`, `#[gpui::test]` in `editor/tests.rs`, driven through the `dispatching`
helper and `simulate_keystrokes` by the keymap rule, one per binding, with
`TestAppContext::{write,read}_from_clipboard` for the clipboard end:

13. `cmd-c` then `cmd-v` round-trips through the real clipboard.
14. `cmd-x` writes the clipboard and removes the text.
15. `cmd-v` of `"a\r\nb"` inserts `"a\nb"`.
16. `cmd-v` with no clipboard item, and `cmd-v` with an item holding `""`,
    both leave the text and the undo stack alone. Two cases, one test, each
    driven through the full type-paste-type-undo sequence: they leave through
    different early exits, so neither covers the other.
17. A whole-line marker we did not write — Zed's JSON under the same pasteboard
    type — pastes as an ordinary span. Added after review: the exact-match rule
    was implemented and unpinned, and every other test round-trips an item
    Inkra wrote itself.
18. `cmd-c` fires **no buffer observer** and does not move `Buffer::version`.
    The observer half is the load-bearing one: routing `Copy` through a
    mutable lease would notify and schedule an autosave while leaving
    `version()` untouched, and only the observer count catches that. The
    pattern is `saving_does_not_report_the_buffer_as_changed`
    (`app/src/vault.rs:750-796`).

## Deferred

| Item | Trigger |
|---|---|
| Telling the input method its composition was abandoned by an edit | An IME user reports a stray candidate. Covers `⌘Z` as much as `⌘X`; fix them together. |
| Per-selection clipboard metadata | Multi-cursor lands. |
| Image paste, attachment files, `ExternalPaths` | Markdown attachments land — the whole flow needs a "write a file beside this note" primitive the vault does not have. |
| Pasting a URL over a selection as a markdown link | Markdown lands. `EditorState::paste` already has what it needs. |
| Duplicate line, move line up/down | No trigger, but they reuse this change's whole-line range exactly; roughly 2 h and 4–5 h once it exists. |
| U+2028 / U+2029 treated as line breaks on paste | A paste is measured to produce one long line where the source showed several. |
| The macOS Edit menu (`MenuItem::os_action`) | The application sets any menu at all; it sets none today. |
| `cmd-shift-v` | Deliberately left unbound. It is not the macOS "paste and match style" chord — that is `opt-shift-cmd-v` — and in VS Code it opens the markdown preview, which is what a user of *this* application is more likely to expect from it. |
| Kill ring (`ctrl-k` / `ctrl-y`) | No trigger. It is a second cut/paste pathway with its own append-on-repeat state (`clipboard.rs:426-512`), not a reuse of this one. |

Not deferred, but flagged: `Buffer::clip` snaps to a char boundary, not a
grapheme boundary (`text/src/buffer.rs:420`), so a mouse-placed caret can sit
inside a ZWJ sequence and a paste will split it. Pre-existing, reachable today
by typing, and owned by `text` rather than by this change.

## Verification

`./scripts/check.sh` green, plus the application run and looked at: `check.sh`
renders nothing, and a paste is one of the few actions whose result is obvious
on screen and invisible to a headless test. The manual pass:

- Paste a line item at a bare caret. Is the caret on the pushed-down line, in
  the column it was in? A stale `DocumentLayout` puts it one row off while
  every offset assertion still passes.
- Paste enough text to push the caret below the fold. Does `reveal_line`
  scroll it back into view?
- Paste one very long unbroken line — a URL, a CSV row. The caret goes off the
  right edge and stays there. See below.
- Copy in Inkra, paste into TextEdit; copy in TextEdit, paste into Inkra. The
  real pasteboard, its custom type and its hash guard are all invisible to
  `TestPlatform`.
- Paste from a genuinely Windows-authored document, not a synthesized `\r\n`.
- After `⌘C`, no dirty dot appears in the sidebar.

### This fires a deferred trigger

The roadmap's horizontal scroll item reads: "A note with lines long enough to
push the caret off the right edge" (`docs/roadmap.md:17`). One `⌘V` of a URL
creates that line in a single keystroke, where typing it takes deliberate
effort. `TextElement::request_layout` reports `width: relative(1.)`
(`app/src/editor/element.rs:267`) and the editor `div` sets only
`.overflow_y_scroll()` (`app/src/editor/mod.rs:282`), so the line clips and the
caret is lost silently. Landing paste does not require fixing it — the roadmap
estimates 1–2 d — but it stops being a theoretical trigger the day this ships,
and `docs/roadmap.md` gains a line saying so in the same change.

## CLAUDE.md changes this owes, in the same change

New rows in "Don't recreate":

| Need | Use | Where |
|---|---|---|
| Copy, cut or paste the selection | `EditorState::copy` / `cut` / `paste`, returning `Copied` | `editor/src/state.rs` |
| Normalize line endings on text entering a buffer from outside — a paste, like a load | `inkra_vault::normalize_line_endings` | `vault/src/fs.rs` |

An exception to the gates paragraph, which currently reads as exhaustive:

> `Copy` is the one action that goes through none of the four gates. It is a
> pure read: it must not notify, because `as_mut` notifies unconditionally and
> an autosave would be scheduled for a keystroke that changed no text. It takes
> `self.buffer.read(cx)` directly. An action that mutates neither the buffer nor
> `EditorState` may follow it; anything else goes through a gate.

A Code rule, generalizing what `Fs::load` guarantees now that a second door
exists:

> `Buffer` assumes LF. Every door text takes to reach one — `Fs::load`, and now
> paste — calls `inkra_vault::normalize_line_endings` first. A third source of
> outside text owes the same call before it reaches `EditorState`.

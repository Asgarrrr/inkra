# Autosave: quit flush, empty state, doc debt — Implementation Plan

**Status: APPROVED, steps 0–2 + 4–5. Step 3 (empty state) split out —
see "Recommendation: split slice 4 into its own plan" below.**

Review outcome: blue team verified V1–V7 CORRECT. Red team returned REVISE with
seven defects, four of which changed this plan. Both teams' conflicts are
arbitrated inline and marked **[arbitrated]**.

Continues `docs/superpowers/handoffs/2026-09-21-autosave.md`. Branch
`vault-open-list-save`, base commit `e4d9a59` (`./scripts/check.sh` green).

**Goal:** close slices 3 and 4 of the autosave feature — flush dirty notes on
quit, and render an empty state when no note is open — plus the test tooling
both need and the documentation debt slices 1 and 2 left behind.

---

## Verified constraints

Every claim below was read in `vendor/zed-reference`, the checkout we compile.
Line numbers are from that checkout at pinned commit `526c95d`.

| # | Claim | Source |
|---|---|---|
| V1 | `Context<T>::on_app_quit` takes `FnMut(&mut T, &mut Context<T>) -> Fut`. The closure body runs **synchronously** with full entity access; only the returned future is deferred. | `gpui/src/app/context.rs:200-218` |
| V2 | `App::shutdown` collects every observer future first, then `windows.clear()`, `window_handles.clear()`, `flush_effects()`, then `block_with_timeout(SHUTDOWN_TIMEOUT, join_all(futures))`. | `gpui/src/app.rs:1057-1082` |
| V3 | Native quit holds `cx.try_borrow_mut()` across the whole `shutdown()` call, so anything re-entering the `App` from a quit future panics on the double borrow. | `gpui/src/app.rs:998-1015` |
| V4 | `TestScheduler::block_until` pushes the blocking session onto `blocked_sessions`; `step_filtered` excludes runnables whose `session_id` is blocked, and treats background runnables (`session_id: None`) as always-candidates. | `scheduler/src/test_scheduler.rs:93-158`, `:341-373` |
| V5 | `TestAppContext::quit()` calls `shutdown()` directly. The `#[gpui::test]` macro instead calls `App::quit()` inside an `update`, which is `self.platform.quit()` — and `TestPlatform::quit` has an empty body. Quit observers therefore fire **only** when a test calls `cx.quit()` itself, and only once. | `gpui/src/app/test_context.rs:231-234`, `gpui_macros/src/test.rs:172`, `gpui/src/app.rs:1111-1113` |
| V6 | `Buffer::version()` is the id of the last edit of the **top** undo transaction. Undo pops that transaction, so the value goes **down**. `version` is not monotonic and cannot order writes. | `crates/text/src/buffer.rs:102-120` |
| V7 | `SHUTDOWN_TIMEOUT` is 200 ms, but `block_until`'s bound under the test scheduler is a random tick count and `advance_clock_to_next_timer` jumps the virtual clock without comparing it to any deadline. The budget is not testable. | `gpui/src/app.rs:75`, `scheduler/src/test_scheduler.rs:110-116`, `:441-448` |

**V6 corrects the handoff.** The handoff proposes "a per-note async writer lock
(a `futures::lock::Mutex`, or equivalent) that both the chain's write and
`flush_all` acquire". A bare lock is **not sufficient**, for two reasons
established below (R1, R2). This plan keeps the lock and adds an ordering key.

---

## Open questions — settled by review

- **Q1 — does the chain's in-flight write survive shutdown? It is a thread
  race, and it frequently resolves in favour of the write surviving.** Quit
  observers run at `app.rs:1060-1062`, **before** `windows.clear()` at `:1064`,
  so `flush_all`'s synchronous half always executes with `NoteStore` alive.
  `NoteStore` is then released before the futures are polled: dropping the last
  strong handle only pushes the id onto `dropped_entity_ids`
  (`entity_map.rs:365-375`); the value is dropped in `release_dropped_entities`
  (`app.rs:1853-1870`, `entity_map.rs:190-209`), which `flush_effects`
  (`app.rs:1781-1784`) loops until empty — so the whole cascade resolves inside
  the `flush_effects()` at `:1066`. **[arbitrated]** Blue team called `AppShell`
  the sole strong holder; red team is right that `Sidebar` holds a second
  (`sidebar.rs:27`), so the cascade is `AppShell` → `Sidebar` → `NoteStore`.
  The conclusion is unchanged, the chain of custody is not.
- **Q2 — a dropped `Task` is cancelled only in the sense of "not polled
  again".** `Drop for Task` sets a cancel flag (`async-task-4.7.1/src/task.rs:440-445`);
  it cannot interrupt a `poll()` already executing. `RealFs::save`
  (`crates/vault/src/real_fs.rs:108-132`) contains **zero** `.await` points, so
  once dequeued it runs write-and-`rename()` to completion in one poll. On
  macOS background work runs on a real GCD queue
  (`gpui_macos/src/dispatcher.rs:39-54`), so a write a worker thread already
  started lands regardless of what the main thread drops.
  **Therefore the R2 race is real in the binary and the `Writer` is necessary.**
  Cancelling the `Task` makes the write's *outcome* unobservable to `NoteStore`;
  it does not make the write un-landed.
- **Q3 — yes, and there is no hang or panic path.** `block_until` polls the quit
  future directly and separately steps background runnables, which are always
  candidates regardless of the blocked session (V4). The panicking arms of
  `park` (`test_scheduler.rs:488-560`) require `deadline: None`; `shutdown`
  always passes `Some(SHUTDOWN_TIMEOUT)`, so the reachable outcome is the
  `deadline.is_some() => false` arm — a clean early return that `shutdown` logs.
  `allow_parking` is already `false` by default (`:764`), independent of the
  `forbid_parking()` the test macro calls in teardown.
- **Q4 — `futures::lock::Mutex` guarantees no fairness at all**, which is
  *stronger* than this plan assumed. `futures-util-0.3.34/src/lock/mutex.rs:14-21`
  documents that tasks may acquire out of request order and that one repeat
  taker can starve the rest indefinitely. `unlock` wakes one arbitrary waiter
  (`:186-194`), and the guard releases by RAII (`:430-433`) — so no guard leaks
  on cancellation, but call order decides nothing. The sequence key is not
  belt-and-braces; it is the only thing that orders the writes.

---

## Step 0 — `MemFs` save gate and failure injector

**Already written, uncommitted, `cargo test -p inkra-vault --all-features`
green (30 unit + 1 proptest + 9 `real_fs`).** Listed so the review covers it.

- `State.blocked` renamed `blocked_loads`; `blocked_saves` added alongside.
- `State.unwritable: BTreeSet<PathBuf>` — every `save` of those paths fails with
  `PermissionDenied`. Sticky, not one-shot: a failed write leaves the note
  dirty, so a one-shot injector would be repaired by the next autosave.
- `LoadGate` renamed `Gate`, now shared by `block_load` and `block_save`. One
  type rather than two identical ones; the three existing call sites in
  `shell.rs` infer it and do not change.
- `MemFs::save` parks on its gate **before** the insert, then checks
  `unwritable`, so a gated write has demonstrably not landed while held.
- Two unit tests in `mem_fs.rs`: the failure injector, and that a blocked save
  has not reached the file until released (polled once with `now_or_never`).

**Why first:** it is the missing tool the handoff names. Without `block_save`
every `MemFs` write completes inside the call that issued it, so no test can
produce two writers of one note; without `make_unwritable` the `SaveFailed` emit
and `AppShell`'s subscription to it can both be deleted with the suite green.

---

## Step 1 — the two tests the handoff left unwritten

**Partially written, uncommitted, and currently BROKEN — it does not compile.**
Both live in `crates/app/src/vault.rs`'s `mod tests`.

```
error[E0599]: no method named `update_window_root` found for struct
`VisualTestContext` in the current scope
   --> crates/app/src/vault.rs:656:14
error: could not compile `inkra-app` (bin "inkra-app" test)
```

`update_window_root` does not exist anywhere in the vendored gpui; it was
written from memory rather than checked, which CLAUDE.md forbids. Because the
whole `inkra-app` test binary fails to build, **no test in `vault.rs` runs right
now**, including the ones slices 1 and 2 recorded as green.

Red team proposed returning `WindowHandle<AppShell>` from `open_shell` and
reaching `shell.notes` through it. That does not work either: `AppShell.notes`
(`shell.rs:20`) is private, and `crate::vault::tests` is a sibling of
`crate::shell`, not a descendant — `shell.rs::open_vault` only gets away with
`shell.sidebar` because that access is inside `shell.rs`. The fix is smaller
than either: `open_shell` **constructs** the `Entity<NoteStore>` itself, so it
should simply return it alongside the `VisualTestContext`. No `AppShell` access,
no new visibility, no widened field.

1. `a_keystroke_after_cmd_s_does_not_cancel_the_write_it_asked_for` — pins the
   `07f1a2c`/`3693f70` cancel fix. Sequence: type `X`; advance
   `AUTOSAVE_DELAY` so the periodic chain reaches its write and parks on a
   `block_save` gate; `⌘S` (installs a second chain, queued behind the first,
   timer already ready); type `Y` (replaces `note.cancel`, dropping the second
   chain's sender → its receiver resolves `Err(Canceled)`); release the gate;
   `run_until_parked` with **no** further clock advance. Expect `beforeXY`.
   Against the unfixed arm the second chain returns on `Err` and the file holds
   `beforeX`.
2. `a_failed_write_reports_it_and_leaves_the_note_dirty` — `make_unwritable`,
   subscribe to `SaveFailed` on the `NoteStore`, type, advance the clock.
   Assert one event, the file unchanged, and `is_dirty` still true.

**Review must confirm** each test fails against the pre-fix code. A test that
passes either way is decoration, which is why the original was deleted.

---

## Step 2 — Slice 3: flush on quit

### The two failure modes

- **R1 — the chain cannot be awaited.** V4: `cx.spawn` is foreground on the
  session that `block_with_timeout` just blocked. Routing the quit write through
  the existing per-note chain deadlocks, because the chain's foreground half can
  never resume during the block. The quit future may only await **background**
  work.
- **R2 — two unordered writers.** With the chain unawaitable, `flush_all` issues
  a second, independent write for the same path. Both `RealFs::save` calls end
  in `rename()`. If the older one lands last the file reverts — the exact loss
  this feature exists to prevent, arriving through its own safety net.

A bare mutex fixes neither ordering problem: `futures::lock::Mutex` grants in
call order, and the flush may call `.lock()` before the already-spawned stale
write is first polled. It would then write the new text first and the stale
write second. Serialisation without ordering just makes the revert tidy.

### Design

```rust
/// Serialises one note's writes and discards one that a newer write overtook.
///
/// Locked and unlocked entirely inside a background task. No foreground code
/// may hold this guard: the quit future is polled while the foreground session
/// is blocked (V4), so a guard held by a `cx.spawn` chain would never be
/// released and shutdown would spin until its timeout.
struct Writer {
    /// The sequence of the newest write that reached the filesystem.
    written: futures::lock::Mutex<u64>,
}
```

- `OpenNote` gains `writer: Arc<Writer>`.
- `NoteStore` gains `next_write: u64`, a plain monotonic counter.
- A sequence is minted **where the text is captured**, under `&mut App`, so
  sequence order is issue order. This is what `Buffer::version` cannot do (V6):
  undo lowers the version, so a flush after an undo carries a *lower* version
  than the write it must beat.
- One shared body, used by both writers:

```rust
async fn write_through(
    writer: Arc<Writer>, fs: Arc<dyn Fs>, path: PathBuf, text: String, seq: u64,
) -> Option<Result<Saved>> {
    let mut written = writer.written.lock().await;
    if seq < *written { return None; }          // a newer write already landed
    let result = fs.save(&path, &text).await;
    if result.is_ok() { *written = seq; }
    Some(result)
}
```

- `NoteStore::write` keeps its existing bookkeeping (`saved_version`,
  `reidentify`, `SaveFailed`) and moves the lock + save inside its existing
  `cx.background_spawn`. `None` means a newer write won; it takes no bookkeeping
  and emits nothing.
- `NoteStore::flush_all`, registered with `cx.on_app_quit` in `NoteStore::new`,
  the `Subscription` held in a new field:
  - **Synchronous half** (V1, and Q1 confirms the entity is still alive here):
    for every open note whose `version() != saved_version`, resolve the path the
    same way `write` does (vault lookup, or `OpenNote.path` for `DETACHED`),
    read the text, mint a sequence, and `cx.background_spawn` a `write_through`.
  - **Async half**: `join_all` over the spawned `Task`s. No `App` access of any
    kind (V3), no bookkeeping — nothing reads `saved_version` after the process
    exits. Spawning in the synchronous half rather than awaiting `write_through`
    inline keeps the guard-only-inside-a-background-task invariant literally
    true and matches what `write` already does (`vault.rs:311-313`). The
    returned `Task`s must be **moved into** the future, never dropped: a dropped
    `Task` is cancelled (Q2).
- `NoteStore::new` gains a `&mut Context<Self>` parameter. **Seven** call sites,
  not the three this plan first claimed — red team's grep is correct:
  `main.rs:100`, `shell.rs:168`, `shell.rs:230`, `sidebar.rs:194`,
  `vault.rs:369`, `vault.rs:437`, `vault.rs:481`. Four need `|_|` → `|cx|`; the
  rest only take the extra argument. The compiler catches every one.

### Tests

Red team refuted the original test 2, on two independent grounds, and both hold:

- **It cannot reproduce production.** `TestScheduler` runs `runnable.run()` on
  the thread driving `block_until` (`test_scheduler.rs:308-417`); there is no
  second thread. The production hazard is a GCD worker mid-`rename()` (Q2). A
  single-threaded scheduler can model "queued but not yet polled", never
  "already executing elsewhere". Worse, for the exact choreography proposed —
  `gate.release()` then `cx.quit()` with nothing polled between — the stale
  write is *queued and unpolled*, which is precisely the case production's
  entity-drop cascade would cancel. The test only produces two live writers
  because the test itself holds a third strong `Entity<NoteStore>` that
  production does not have.
- **"Whichever lands first" is one arbitrary seed, forever.** `#[gpui::test]`
  with no `iterations` runs seed 0 only, and `check.sh` runs plain
  `cargo test --workspace` with no `SEED`/`ITERATIONS`. The test would not be
  flaky; it would silently pin one interleaving and could start passing
  vacuously against a broken guard if gpui's scheduling weights ever change.

**Replacement: test the ordering guard directly and deterministically.**
`write_through` is a free async function over `Arc<Writer>` and `Arc<dyn Fs>`
with no gpui in it, so a plain `#[test]` with `futures::executor::block_on` can
drive **both** interleavings by construction instead of hoping the scheduler
produces them. This is stronger than the gpui test it replaces, and it is the
`scroll.rs` precedent CLAUDE.md already allows: pixel-free, window-free, and
unable to move down a layer because it needs both `Fs` and the sequence.

1. `a_later_write_wins_when_it_lands_second` — seq 1 then seq 2, file holds
   text 2.
2. `a_later_write_wins_when_it_lands_first` — seq 2 then seq 1, file **still**
   holds text 2; the stale call returns `None` and never touches the file. This
   is the R2 revert, forced rather than raced.
3. `a_stale_write_is_skipped_rather_than_failed` — `None`, not `Some(Err(_))`,
   so the caller does not emit `SaveFailed` for a write that was merely
   overtaken.

Then, `#[gpui::test]` in `crates/app/src/vault.rs`, driving quit with `cx.quit()`
(V5), claiming only that the flush is wired up and completes:

4. `quitting_writes_every_dirty_note` — two notes open and dirty, only one
   `current`. Quit. Both files hold their buffer's text. Pins that the flush is
   not `save_current` under another name.
5. `quitting_leaves_a_clean_note_alone` — the version comparison, at quit.

**The 200 ms budget is not asserted** (V7), and the doc comments must say so
rather than implying a timing guarantee. Nor may any doc comment claim test 4
or 5 reproduces the concurrent race — tests 1–3 own that, by construction.

---

## Step 3 — Slice 4: empty state

Decided with the user; recorded in the handoff. When no note is open, render an
empty state instead of an editable buffer.

Closes the unnamed-launch-buffer hole: `Launch::Nothing` builds a `Buffer` that
never reaches `adopt`, so it gets no observer, no sidebar row, no quit flush,
and `⌘S` silently does nothing. Autosave makes that worse by teaching the user
not to press `⌘S` at all. It also covers `Launch::Vault`, which currently shows
an anonymous empty buffer until a row is clicked.

- `AppShell.editor` becomes `Option<Entity<EditorView>>`, built on the first
  `open_note`.
- `AppShell` gains its own `FocusHandle`; `Focusable::focus_handle` returns the
  editor's when there is one and its own otherwise. Today it delegates
  unconditionally.
- `main.rs` stops building a buffer and an editor for `Launch::Nothing` and
  `Launch::Vault`. `Launch::Note` still builds both, and still `adopt`s.
- **Text only.** Both actions an empty state would want are blocked: "open a
  vault" needs `prompt_for_paths` (deferred, `gpui/src/app.rs:1687`), "create a
  note" needs the create/rename/delete slice. The user deferred the picker
  explicitly.
- Tests: launching on nothing renders the empty state and `⌘S` is a no-op rather
  than a panic; activating a note replaces it with an editor; the `cmd-s` and
  keystroke tests rooted at `AppShell` still pass through the new `Option`.

### Two defects review found here, neither of which this plan had solved

**F1 — nothing can move focus to the first editor.** With `editor` optional, the
window is focused on `AppShell`'s own handle at launch. When the first note is
activated `open_note` builds the editor — but `open_note` (`shell.rs:73-82`)
takes only `&mut Context<Self>`, it is reached from a `cx.subscribe` callback
that `shell.rs:42-43` documents as having no `Window`, and it finishes inside a
`cx.spawn`. `AppShell` stores no window handle. **There is no path in the plan
that could call `window.focus(...)`.** `EditorView` routes text input through
`track_focus` (`editor/mod.rs:257`), so the symptom is: click a row, the editor
appears, `cmd-s` still works because action dispatch walks focused-node-to-root
— **and typing does nothing**. `check.sh` cannot catch this (it renders
nothing), which makes the `cargo run` eyeball pass load-bearing here rather than
hygiene.

The fix is a real design change, not a patch: `AppShell::new` must take the
`&mut Window` that `main.rs`'s `cx.open_window(opts, |window, cx| …)` closure
already receives and currently discards (`main.rs:90`), subscribe with
`subscribe_in`, and open notes with `spawn_in` so the async half holds an
`AsyncWindowContext` that can focus the new editor.

**F2 — the test helper's contract inverts.** `shell.rs::open_vault`
(`:156-179`) hands callers a ready-made `Entity<EditorView>` as a
*precondition*. Under an optional editor there is no editor until the first
activation, so it becomes a *postcondition* reachable only through a new
accessor. Four tests depend on the current shape:
`clicking_a_row_hands_its_buffer_to_the_editor`,
`an_abandoned_load_does_not_supersede_a_later_activation`,
`the_current_note_is_the_one_in_the_editor`, and
`an_activation_records_the_note_only_once_the_editor_holds_it`. Plus
`Focusable::focus_handle` (`:92-96`) and `Render` (`:98-134`) both branch.

### Recommendation: split slice 4 into its own plan

Slice 3 is fully specified and its uncertainty is now retired. Slice 4 is not:
F1 is an unsolved focus design and F2 is a test-contract rewrite. Bundling them
means the quit flush — which stops real data loss — waits on a UI question that
has nothing to do with it.

The ordering argument that favoured slice 4 first was avoiding a double touch of
the `AppShell`-rooted tests. That cost is small: slice 3's tests 1–3 root at
nothing, and tests 4–5 use `open_shell`/`open_two_notes`, which build
`NoteStore` directly. Only `open_shell` touches `AppShell` at all.

**This plan therefore proposes shipping steps 0–2 and 4–5, and moving step 3 to
a separate plan with its own review.** That is a deliberate scope reduction and
is the user's call to accept or reject, not this plan's.

---

## Step 4 — documentation debt

Nothing here is started. Listed in the handoff; restated so it is trackable.

- `docs/superpowers/specs/2026-09-21-autosave-design.md` — wrong in four places
  and to be rewritten or marked superseded: `⌘S` semantics (needs the cancel
  channel), the hard-link claim (inverted — replacing a linked note splits the
  pair), "a failure at any step removes the temp file" (that is prose; it is now
  an RAII guard), and `sync_all` described as making the rename durable (it does
  not — that needs the parent directory fsynced, which neither we nor Zed do).
- `CLAUDE.md` — the `NoteStore::current` paragraph reads as though `current`
  gates every write. It no longer does: the periodic writer writes every dirty
  note regardless, and `current` only decides what `⌘S` targets and which note a
  switch writes. Add the quit flush, `Writer`, `Gate`, `block_save` and
  `make_unwritable` to the don't-recreate table; `LoadGate` there is now `Gate`.
- `docs/roadmap.md` — "ships open, list and save" is now false.
- `docs/superpowers/specs/2026-09-20-inkra-vault-design.md` Deferred table —
  close "Atomic save"; narrow "Serialized writes per note" to the cross-note
  case and drop its "needs a delay hook in `MemFs`" note, which `block_save`
  answers; update "Conflict detection" (its premise that `NoteStore` carries no
  identity is stale); rewrite "Save the unnamed launch buffer"'s trigger, which
  describes a failed `⌘S` the user would notice and should describe silent
  timer-driven loss; append to "Buffer eviction" that evicting now also means
  cancelling the pending chain and dropping the observer `Subscription`.

---

## Step 5 — `/comment-cleanup`

Run once, over the whole feature, after step 3. Never run on any of this code.
Once at the end rather than per slice, so the comments are rewritten once.

---

## Verification

`./scripts/check.sh` green is the definition of done, plus the eyeball pass
CLAUDE.md requires after touching anything that renders — slice 4 changes what
the window shows when launched with no argument, so `cargo run -p inkra-app`
with and without a path is mandatory, not optional.

## Known gaps this plan does **not** close

Added by review: **a timed-out shutdown does not stop the process exiting.**
`App::shutdown` logs `"timed out waiting on app_will_quit"` and sets
`self.quitting = false` unconditionally (`app.rs:1073-1081`); it neither retries
nor blocks termination. A vault on a slow network volume can therefore lose the
flush write mid-flight no matter how correct the `Writer` is. The 200 ms budget
is a best-effort, and nothing in this plan changes that.

Also: **buffer eviction will have to cancel the `Arc<Writer>` too.** Not
reachable today — `open_note` short-circuits on an already-open id, so a
`Writer` is built once per `EntryId` per session — but the doc-debt bullet about
eviction cancelling the chain and dropping the observer now needs a third item.

Carried forward from the handoff, unchanged: no conflict detection (writing
every 2 s over a file a sync client just changed is a silent clobber — the
largest honest hole); the modified dot cannot distinguish "typing" from "saving
has failed for an hour"; a crash leaks a `*.tmp` beside the note with no sweep;
the atomic write loses owner, group and ACLs; the write now needs write+execute
on the parent directory where the truncating write it replaced did not.

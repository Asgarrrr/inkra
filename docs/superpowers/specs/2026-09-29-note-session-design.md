# Note session: one module owns a note's lifecycle in `NoteStore`

Status: **implemented** (2026-09-29). A refactor: behaviour stays identical,
except the pruning of dormant sessions (invariant 10). See "As built".

## Why

`NoteStore` is one type split into impl blocks over seven files in
`crates/app/src/vault/`. One note's state sits in three maps whose keys must
agree:

- `open: HashMap<EntryId, OpenNote>`: buffer, `location`, `saved_version`,
  `writer` (`store.rs:17-24`).
- `ops: HashMap<EntryId, Ops>`: `chain`, `scheduled`, `cancel`, `trashing`
  (`ops.rs:7-21`). Not pruned on close.
- `observers: HashMap<EntryId, Subscription>`: one per open note
  (`store.rs:237-263`).

The rules over that state are spread across the files that read it:

- `ops.trashing` is read by hand five times: `claims` (`gone.rs:14`),
  `check_open_notes` (`reload.rs:28`), the reload's first step
  (`reload.rs:64`), `flush_all` (`save.rs:79`), and `trash` (`tree_ops.rs:32-63`,
  set, tested, and cleared on failure).
- `Location` flips in one place, `check_open_notes` (`reload.rs:14-51`). The
  brief also names `reclaim_gone` in `gone.rs`; the scan-session slice moved
  it into `inkra_vault::scans::claims`, where it moves an entry's id and never
  touches `Location`. `gone.rs` now only builds `Claims`.
- Callers must know the chain protocol. `rename` and `trash` call
  `schedule_save(id, ZERO)` before `after_chain` (`tree_ops.rs:42`, `:160`),
  and `trash` alone chooses `cancel_pending` for a gone note
  (`tree_ops.rs:38`). The vault rule file has to state both, because no
  type does.
- `saved_version` and the `Writer`'s stamp move together, but in two places:
  `write` sets `saved_version` after `write_through` set the stamp
  (`save.rs:165`), and `reload` sets `saved_version` in `did_reload`, then
  must remember to `adopt` the stamp (`reload.rs:117`).

Nothing but convention stops a new caller from reading `location` while
ignoring `trashing`, or from queuing a rename that skips the preempting write.

## What Zed does

Zed keeps a buffer's disk state on the `Buffer`: `DiskState` is
`New | Present { mtime, size } | Deleted | Historic`
(`language/src/buffer.rs:406-416`). `did_save` and `did_reload` set
`saved_version` and `saved_mtime` in one call (`buffer.rs:1636-1649`,
`:1760-1774`), so the pairing is one method. `Project::delete_entry`
(`project/src/project.rs:2747`) hands the delete to the worktree and does not
order it against a pending autosave.

**Taken:** one type owns the lifecycle, as an enum; one call updates each
pairing.

**Deviation:** the state lives in the store, not on the buffer. `inkra-text`'s
`Buffer` has no file and no gpui (CLAUDE.md, architecture), and a
`DETACHED` note shares no vault.

**Deviation, kept from the conflict spec:** the stamp stays in `Writer`, behind
its lock, not beside `saved_version`. A second write issued before the first
reaches the lock would otherwise preserve its own text into a conflict file
(`writer.rs:36-38`).

**Deviation, kept from the create-rename-delete spec:** a trash is a state of
the note, ordered on its chain. Zed does not order a delete against a save;
our autosave writes unprompted, so a late write would recreate the file.

## The deepened module: `vault/session/`

New directory `crates/app/src/vault/session/`, beside `store.rs`. It owns
everything the store keeps per `EntryId`.

```
session/mod.rs         NoteSession, OpenNote, the store-facing methods
session/lifecycle.rs   Lifecycle<B>, Location, PendingSave, Swap: no gpui, no Buffer
session/lifecycle/tests.rs
session/queue.rs       Queue (was ops.rs): the chain and the pending save
session/writer.rs      moved from vault/writer.rs, unchanged
session/writer/tests.rs
```

`lifecycle.rs` imports neither gpui nor `inkra_text`. It stays in `app`, not in
`inkra-vault`: it is the policy of the binding between a file and a buffer,
which CLAUDE.md places in `NoteStore`. `inkra-vault` would have to export
the trash and preempt rules of a store it never sees. The deviation from
"a test that needs no `Window` stays out of `app`" is recorded below;
`writer/tests.rs` is the precedent.

### What it hides

- One map: `sessions: Sessions` replaces `open`, `ops` and `observers`. The
  `Subscription` moves into `OpenNote`, so closing a note drops its observer
  in the same move.
- The live set: the ids whose lifecycle holds an `OpenNote`. A swap runs on
  every autosave (the watcher sees the final rename of the atomic save, F3
  filters only the temp file), so its loops iterate this set, never every
  session.
- Pruning: a closed session with no queued work is dropped.
- The trash phase, as a state of the lifecycle, not a bool read by callers.
- The `Location` transitions at a swap (F7, F9).
- The chain protocol: what happens to the pending save before a queued
  operation.
- The `saved_version` and stamp pairing, on each landed write and applied
  reload.

### Interface

```rust
// session/lifecycle.rs — B is `OpenNote` in the store, `()` in the tests.

/// Where an open note's file is (watcher spec, F9). Unchanged.
pub(super) enum Location { Vault, Detached(PathBuf), Gone(PathBuf) }

/// One id's lifecycle. Replaces `open.contains_key` plus `Ops::trashing`.
pub(super) enum Lifecycle<B> {
    Closed,
    Open(Location, B),
    /// Our trash is queued or done. `Some` while the note is open.
    /// Sticky on success: a second trash is refused, as today.
    Trashing(Option<(Location, B)>),
}

/// What a queued operation does to the note's pending save.
pub(super) enum PendingSave {
    /// Preempt: write now, ahead of the operation (rename, trash of a present note).
    Write,
    /// Cancel: the write would recreate a file the operation discards (trash of a gone note).
    Drop,
    /// Leave it (a reload: it runs on clean notes only).
    Keep,
}

/// What one swap did to one open note.
pub(super) struct Swap { pub flipped: bool, pub reload: bool }

impl<B> Lifecycle<B> {
    pub fn open(&mut self, location: Location, bound: B);   // Closed→Open, Trashing(_)→Trashing(Some)
    pub fn bound(&self) -> Option<&B>;                      // Open and Trashing(Some)
    pub fn location(&self) -> Option<&Location>;
    pub fn is_gone(&self) -> bool;                          // Trashing(Some(Gone)) too, as today
    /// F7 and F9's detect step. `now`: the live entry's mtime; `before`: the
    /// replaced state's entry mtime; `last`: `previous.absolute(id)`.
    pub fn swapped(&mut self, now: Option<Mtime>, before: Option<Mtime>,
                   last: impl FnOnce() -> Option<PathBuf>) -> Swap;
    /// `None`: not a claimant (closed, trashing). `Some(kept)`: `Claims.claimants`.
    pub fn claim(&self) -> Option<Option<PathBuf>>;
    /// The quit flush and a reload's first step act only on `Open`.
    pub fn is_open(&self) -> bool;
    /// `None` refuses the trash; else the note is `Trashing` and the answer says
    /// what to do with its pending save.
    pub fn begin_trash(&mut self, has_entry: bool) -> Option<PendingSave>;
    pub fn trash_failed(&mut self);                         // Trashing(x) → Open(x) or Closed
    pub fn trash_done(&mut self) -> Option<B>;              // Trashing(Some) → Trashing(None)
    /// With no vault entry at the trash's step: a gone note's last path.
    pub fn last_write(&self) -> Option<(&PathBuf, &B)>;
}

// session/queue.rs — gpui. The fields of today's `Ops`, private.
pub(super) struct Queue { chain: Option<Task<()>>, scheduled: bool,
                          cancel: Option<oneshot::Sender<()>> }

// session/mod.rs
pub(super) struct NoteSession { queue: Queue, life: Lifecycle<OpenNote> }

/// Every session, and the live set: the ids whose `life.bound()` is `Some`.
pub(super) struct Sessions { all: HashMap<EntryId, NoteSession>, live: HashSet<EntryId> }

impl Sessions {
    fn get(&self, id) -> Option<&NoteSession>;
    fn note(&self, id) -> Option<&OpenNote>;             // life.bound()
    fn note_mut(&mut self, id) -> Option<&mut OpenNote>;
    fn queue(&mut self, id) -> &mut Queue;               // creates a closed session
    /// The one writer of a lifecycle: keeps `live` in step, then drops the
    /// session if it is `Closed` with an idle queue.
    fn update<R>(&mut self, id, f: impl FnOnce(&mut Lifecycle<OpenNote>) -> R) -> R;
    /// A link finished: the tail clears the chain, and an idle closed session goes.
    fn settle(&mut self, id, link: u64);
    /// Over the live set only; `swapped` never changes whether a note is bound.
    fn swapped(&mut self, mtimes, last) -> Vec<(EntryId, Swap)>;
    /// `Claims.open` is the live set, never `all.keys()`: a closed or trashed
    /// session must not keep a reclaim off its id. `claimants` is the live
    /// set filtered through `Lifecycle::claim`.
    fn claims(&self) -> Claims;
    fn live(&self) -> impl Iterator<Item = (EntryId, &NoteSession)>;
}
pub(super) struct OpenNote { buffer: Entity<Buffer>, saved_version: u64,
                             writer: Arc<Writer>, _observer: Subscription }

/// Returned by an applied reload; the stamp moves only when it is awaited.
#[must_use]
pub(super) struct Adopt(Arc<Writer>, Stamp);
```

`NoteStore` keeps two private entry points over a session, and every
per-note operation goes through them:

```rust
fn schedule_save(&mut self, id, delay, cx);                       // autosave, ⌘S, set_current
fn queue(&mut self, id, pending: PendingSave, op, cx) -> Task<bool>; // was after_chain
```

`queue` applies `pending` first (`Write`: `schedule_save(id, ZERO)`, a no-op
for a closed note, as today; `Drop`: `Queue::cancel_pending`; `Keep`:
nothing), then chains `op`. `rename` passes `Write`, `trash` passes what
`begin_trash` returned, and the swap's reload passes `Keep`. No caller calls
`cancel_pending` or `schedule_save(ZERO)` before chaining.

### Invariants, each held by a type or one function body

1. **Open means a buffer.** `Lifecycle::Open` and `Trashing(Some)` carry the
   `OpenNote`; there is no `Location` without one, and no observer without one.
2. **Trashing is one state.** The five hand checks become `claim`, `is_open`,
   `swapped` and `begin_trash`. `write` still runs for a trashing note: the
   preempted write of a present note must land before the delete.
3. **Only `begin_trash` enters `Trashing`,** and it refuses `Detached`, a
   note already trashing, and a closed or present note with no entry. So no
   `Detached` note is ever trashing.
4. **`swapped` is today's loop body, whole.** No entry: a `Trashing` or
   gone note stays; an `Open(Vault)` note becomes `Gone(last)` and flips,
   unless `last` is `None`. An entry: a gone note becomes `Vault` and flips,
   **even while `Trashing`**, as `reload.rs:37-40` does today (its own save's
   re-key can bring the entry back mid-trash). `reload` when `before` is
   `None` or differs from `now`, in every phase; the reload's first step then
   stops on `!is_open`. `Detached` returns nothing.
5. **Sticky trash.** `trash_done` leaves `Trashing(None)`. `trash_failed`
   restores the exact prior state. A load that completes into `Trashing(None)`
   opens as `Trashing(Some)`, as `track` beside a set flag does today.
6. **`NoteClosed` is emitted on every successful trash**, open or not, as
   `tree_ops.rs:75` does today.
7. **The pairing is two methods.** `OpenNote::landed(version)` is the only
   writer of `saved_version` after a write; `OpenNote::did_reload` returns
   `Option<Adopt>`, and `Adopt::run` is the only caller of `Writer::adopt`.
   `flush_all` still sets neither: the app is quitting.
8. **The `scheduled` flag keeps its current semantics.** Any chain step that
   passes its timer clears it, even when a newer delayed save is queued
   behind it (`save.rs:47-51`). That costs at most one extra write; changing
   it would change when autosaves land, which this refactor does not.
9. **The live set is `life.bound().is_some()`, held by `Sessions::update`.**
   Every lifecycle change goes through it. `check_open_notes`, `claims` and
   `flush_all` iterate the live set; their cost stays O(open notes), not
   O(every id ever renamed or trashed).
10. **A closed idle session is pruned.** `Closed` with no chain left: no
   queued rename or trash, no pending save (a closed note never schedules
   one). The last link to finish clears the chain (`Queue::finish`, by link
   number, so a link with another queued behind it keeps the chain).
   `Sessions::update` and `settle` then drop the session. A later operation
   finds no chain to wait on, which is correct: the last one finished.
   `Trashing(None)` is not pruned: the sticky refusal of a second trash
   stays (invariant 5).

## What moves where

| From | To |
|---|---|
| `store.rs`: `OpenNote`, `Location`, `is_gone`, `folder`, `snapshot`, `did_reload`, `writer` | `session/mod.rs` (`OpenNote`) and `session/lifecycle.rs` (`Location`); `folder` becomes `Location::folder` |
| `store.rs`: `open`, `ops`, `observers` fields; `track` | one `sessions` field; `track` calls `Lifecycle::open` |
| `ops.rs` | `session/queue.rs` as `Queue`; `ops.rs` goes |
| `tree_ops.rs`: `after_chain`, the preempt and cancel choice, the `trashing` flag | `NoteStore::queue`, `PendingSave`, `begin_trash`/`trash_failed`/`trash_done` |
| `tree_ops.rs`: `trash_target`'s `Location::Gone` match | `Lifecycle::last_write` |
| `reload.rs`: the flip logic of `check_open_notes` | `Lifecycle::swapped`; the store loop collects `Swap`s, then emits and queues |
| `gone.rs`: the `trashing` filter and `Location` match | `Lifecycle::claim`; `gone.rs` goes, `claims` joins `scans.rs`, its only caller |
| `save.rs`: `saved_version = version`; `flush_all`'s `trashing` skip | `OpenNote::landed`; `Lifecycle::is_open` |
| `writer.rs`, `writer/tests.rs` | `session/writer.rs`, `session/writer/tests.rs`, unchanged |

`store.rs`, `save.rs`, `reload.rs` and `tree_ops.rs` keep the gpui work: the
spawns, the background loads and writes, the events, and `rescan`.

## Tests

The new interface is the test surface. `session/lifecycle/tests.rs` holds
plain `#[test]` cases on `Lifecycle<()>` and one proptest. Each is written
first, red against a stub.

| Behaviour | Criterion (new `#[test]`) |
|---|---|
| A present note deleted on disk goes gone at its last path | `Open(Vault)`, `swapped(None, Some(t), ‖Some(p))`: `Gone(p)`, `flipped`, no `reload` |
| No last path, no gone | same with `last` → `None`: `Open(Vault)`, nothing |
| A gone note whose entry returns comes back and reloads | `Open(Gone(p))`, `swapped(Some(t), None, _)`: `Vault`, `flipped`, `reload` |
| Our own trash is never gone | `Trashing(Some(Vault))`, `swapped(None, …)`: unchanged, nothing |
| A trashing gone note whose entry returns flips back | `Trashing(Some(Gone(p)))`, `swapped(Some(t), None, _)`: `Trashing(Some(Vault))`, `flipped`, `reload` (invariant 4) |
| A detached note is outside the swap | `Open(Detached(p))`: every `swapped` input returns nothing and keeps the state |
| Only a changed mtime reloads | `Open(Vault)`: `before == now` → no `reload`; differs or `None` → `reload` |
| The trash choice | `Open(Vault)`+entry → `Write`; no entry → `None`, state kept; `Open(Gone)` → `Drop`; `Closed`+entry → `Write`, `Trashing(None)`; `Trashing(_)` → `None`; `Detached` → `None` |
| A failed trash restores the note | each of the four `begin_trash` successes, then `trash_failed`: the prior state |
| A done trash is sticky and closes | `trash_done` on `Trashing(Some(x))` returns `x`'s payload, leaves `Trashing(None)`; `begin_trash` then `None` |
| A load during a trash opens trashing | `Trashing(None)`, `open(Vault, ())`: `Trashing(Some(Vault))`, `is_open` false |
| Claims | `Open(Gone(p))` → `Some(Some(p))`; `Open(Vault)`, `Open(Detached)` → `Some(None)`; `Trashing`, `Closed` → `None`; `bound()` is `Some` for `Trashing(Some)` |
| A dormant session is outside `Claims.open` (gpui) | a trashed note's `Trashing(None)` session and a renamed-then-settled closed id: neither is in `claims().open`; the open note is |
| The live set stays the open notes (gpui) | open one note, rename three unopened notes, trash a fourth, run a swap: `live` holds exactly the open note; the renamed ids' sessions are pruned |
| The trash target of a gone note | `last_write` is `Some` for `Trashing(Some(Gone(p)))` only |
| Proptest: no event sequence reaches a wrong state | random `open`, `swapped`, `begin_trash`, `trash_failed`, `trash_done` from `Closed`: no `Trashing` holds `Detached`; `flipped` is true exactly when `is_gone` changed; after `trash_done`, `claim` is `None` until `trash_failed`, which never follows it |

The gpui tests stay, unchanged in meaning. Four reach into fields that go,
and switch to accessors on `NoteStore`, `#[cfg(test)]`:

- `vault/tests/reload.rs`: `saved_version`, `expected` (`notes.open[&id]`) and
  `a_reload_schedules_no_autosave` (`notes.ops[&a].scheduled`).
- `vault/tests/gone.rs:102`: `notes.open[&a].location`.

The pairing (invariant 7) and the chain protocol keep their existing gpui
proofs: `vault/tests/reload.rs` (the expected stamp after a reload),
`autosave.rs` (`a_keystroke_after_cmd_s_does_not_cancel_the_write_it_asked_for`),
`gone.rs` (`trashing_a_dirty_gone_note_closes_it_and_writes_nothing`,
the preempted-write trash), `trash.rs` and `quit.rs`.

Gates: `./scripts/test-names.sh` before and after, diffed: only additions.
`cargo mutants` on `session/lifecycle.rs` with zero missed.
`./scripts/check.sh` green after each step.

## Deviations

- From Zed: listed above (state in the store, stamp in `Writer`, trash as a
  state).
- From CLAUDE.md, Testing: `lifecycle/tests.rs` is a gpui-free `#[test]` file
  inside `app`. The type is the app's binding policy; moving it down would
  make `inkra-vault` hold a store's trash and preempt rules. `Lifecycle<B>` is
  generic with one real payload and one test payload, the seam CLAUDE.md
  accepts (`Fs` with `RealFs`/`MemFs`).

## Risks

- **The trashing gone note's flip.** Freezing `Location` while `Trashing`
  would look cleaner and would drop a `GoneChanged` and change `is_gone`
  during the trash window. Invariant 4 keeps today's flip; a test pins it.
- **The `scheduled` quirk.** A reviewer may "fix" invariant 8 with a
  generation token. That changes autosave timing on the data-loss path, and
  is out of scope.
- **Dropping the observer inside a chain step.** Today `observers.remove`
  runs in the trash's final `update`; `trash_done` returning the `OpenNote`
  drops it at the same point. The review checks the drop happens inside that
  `update`, not after `NoteClosed`.
- **Borrowing.** `check_open_notes` mutates sessions while reading the vault
  entity. It collects `(id, Swap)` first, then emits and queues, as today.
- **Pruning is new behaviour.** `ops` was never pruned. A closed session is
  dropped only once its chain has finished, and it holds nothing else, so no
  test observes the change. The data-loss tests stay unchanged in meaning.
- **Test accessors.** Four tests change the path they read, not what they
  assert; the review diffs their assertions.

## Plan

Each step builds, passes `./scripts/check.sh`, and is one commit.

1. **`Lifecycle` owns location and trash.** Write `lifecycle/tests.rs` red
   against a stub, then `lifecycle.rs`. Merge `open`, `ops` and `observers`
   into `sessions: HashMap<EntryId, NoteSession>`, with `Ops` still inside
   under its old name. Replace the five `trashing` reads, `check_open_notes`'s
   flip logic, `claims`' filter and `trash_target`'s match with the lifecycle
   methods. Switch the four test accessors. Criterion: every lifecycle test
   green, every vault and shell test green, `test-names.sh` diff shows only
   the new names.
2. **`Queue` and `PendingSave`.** Move `ops.rs` to `session/queue.rs` as
   `Queue`; add `NoteStore::queue(id, pending, op)` and route `rename`,
   `trash` and the reload through it. Criterion: `grep` finds no
   `cancel_pending` or `schedule_save(.*ZERO` outside `save.rs`, `store.rs`
   (`set_current`) and `queue`; `autosave.rs`, `trash.rs`, `gone.rs` and
   `tree_ops` tests green.
3. **The pairing.** `OpenNote::landed` and `Adopt`; `write` and `reload` use
   them. Move `writer.rs` under `session/`. Criterion: `reload.rs`'s
   stamp tests and `writer/tests.rs` green; `grep` finds `saved_version =`
   and `.adopt(` only in `session/mod.rs`.
4. **Documents and mutants.** Run `cargo mutants` on `session/lifecycle.rs`,
   zero missed. Update `.claude/rules/vault.md` (`Ops::cancel_pending`,
   `Ops::trashing` and the preempt rule now name `PendingSave` and
   `Lifecycle::Trashing`), the file-watcher spec's F7 and F9 file citations,
   CLAUDE.md's spec list, and this spec's status with an "As built" section.

## As built

Deviations from the plan above, each with its reason:

- **`claim` split into `is_open` and `gone_path`.** `Option<Option<PathBuf>>`
  fails clippy's `option_option`. A claimant is exactly an `Open` note, and
  its kept path is its `Gone` path, so two existing questions answer it.
- **`claims` lives on `Sessions`,** not in `scans.rs`. It reads only the
  live set and the lifecycles; `scans.rs` calls `self.sessions.claims()`.
- **`Sessions` owns the live set and the pruning** (invariants 9 and 10).
  Only `update` changes whether a lifecycle binds a note; `swapped` walks
  the live set and moves a bound note's `Location`, never its binding. `live()` indexes `all` by a
  live id: a miss is a bug in `update`, so it panics.
- **`NoteStore::link`** (in `session/queue.rs`) appends every link, the
  save's and `queue`'s, and settles it when its body returns. A save link
  keeps its timer ahead of the `previous.await`, as before. The finished
  last link clears the chain by `detach`, not drop: it is the running task.
- **`Queue::arm` and `timer_passed`** hold the `scheduled` and `cancel`
  logic of `schedule_save`; `Queue`'s fields are private to `queue.rs`.
- **`OpenNote::unsaved`** holds the write's finalize-and-compare step, so
  `saved_version` is private; `OpenNote::new` takes the stamp and builds the
  `Writer`.
- **`trash` drops its `EntryId::DETACHED` check.** `begin_trash` refuses a
  `Detached` note, and `DETACHED` has no vault entry, so a closed `DETACHED`
  is refused too. The proptest models that fact; invariant 3 depends on it.
- **No `NoteStore::session`.** Callers read `self.sessions.get(id)`; a
  wrapper over one map lookup would add a name and no invariant.
- **`Lifecycle::bound_at`** returns the location and the bound note from one
  match, so a caller never holds one without the other. **`Swap::NONE`** is
  the swap that changed nothing. **`Queue::push`** appends a link and numbers
  it, so `NoteStore::link` never touches `chain` or `links`; the map's
  accessor is `Sessions::queue_mut`, apart from `NoteStore::queue`.
- **Test accessors are on `Sessions`,** not `NoteStore`: `note(id)`,
  `get(id)`, and `#[cfg(test)]` `is_scheduled`, `ids` and `live_ids`.
- **`check_open_notes` reads `previous.entry(id)` for every live note,** not
  only for one with a live entry: one hash lookup per open note.
- **Tests added** beyond the table: `vault/tests/sessions.rs` (the live set,
  pruning, a queued rename keeping the chain) and, from `cargo mutants`,
  `a_second_keystroke_does_not_write_before_the_delay` in `autosave.rs`: an
  inverted `scheduled` in `Queue::arm` survived every existing test.
- **Mutants:** `cargo mutants -p inkra-app --test-package inkra-app` on
  `session/{lifecycle,mod,queue}.rs`: zero missed after that test.

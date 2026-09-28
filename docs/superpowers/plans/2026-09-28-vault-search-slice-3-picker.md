# Vault search, slice 3: the picker — Implementation Plan

**Spec:** `docs/superpowers/specs/2026-09-28-vault-search-integration-design.md`,
S3 and the key of S4. Also the search spec's D7–D9
(`2026-09-24-search-design.md`) and the word-start spec.

**Goal:** `cmd-shift-f` opens a picker over the vault's text. Each keystroke
runs one `lookup`, `search` per `chunks(num_cpus)` and `Searched::merge`
(limit 1,000), off the foreground. Rows are built when the results land:
a header per note, a row per hit. Confirming a hit opens its note at the top.

**Branch:** `vault-search-3-picker` from `vault-search-2-index` (`db88994`).

## Success criterion

Written first, failing to compile on `db88994` (no `vault_search`, no
`SearchVault`):

- `crates/app/src/vault_search/tests.rs`, headless against `MemFs`, the
  picker in its own window as `quick_open/tests.rs` does:
  - `typing_lists_a_header_then_the_hits_of_each_note`: rows read as
    `# a (2)`, `1: …`, `3: …`, `# b (1)`, `2: …`; the first hit is selected.
  - `up_and_down_skip_headers_and_stop_at_the_ends` (`simulate_keystrokes`).
  - `an_empty_field_shows_nothing` (no rows, no empty text) and
    `blank_or_bracket_text_runs_no_lookup`: `"   "` and `"[["` start no scan
    (`runs.started()` unchanged) and say "No matches".
  - `a_query_with_no_match_says_so`: "No matches".
  - `no_match_while_indexing_says_how_far`: a held load gives
    "No matches in 1 of 2 notes indexed so far".
  - `a_note_past_the_per_note_cap_shows_n_plus`: 150 hits → header `100+`.
  - `past_the_limit_the_footer_says_so`: 1,100 hits → 1,000 hit rows,
    footer "1,000+ results; refine the query".
  - `a_stale_result_is_dropped`: `apply` with `runs.id().previous()`.
  - `any_core_count_lists_the_same_rows`: `set_num_cpus` 1, 3, 7.
  - `trashing_a_listed_note_removes_its_header_and_hits`.
  - `a_refresh_during_loads_keeps_the_selected_hit`: a held load lands a note
    before the selected hit's note; after `BUILDING_REFRESH` the same
    `(note, range.start)` is selected.
  - `the_picker_says_scanning_then_lists_after_the_first_build`.
  - `enter_confirms_the_selected_hit`: emits `HitActivated` with its note and
    range.
  - labels: a header's `row_label` is "`name`, `N` matches", a hit's is
    "line `N`: `context`".
- `crates/app/src/shell/tests/vault_search.rs`, rooted at `AppShell`:
  - `cmd_shift_f_opens_vault_search_and_again_closes_it`.
  - `cmd_shift_f_replaces_quick_open`.
  - `cmd_shift_f_does_nothing_without_a_vault`.
  - `enter_opens_the_note_of_the_selected_hit`.
  - `enter_typed_before_the_results_land_opens_the_first_hit`.
  - `escape_dismisses_and_restores_focus`.
- `./scripts/check.sh` exits 0; six shots rendered and looked at.

## Files

| File | Change |
|---|---|
| `crates/app/src/vault_search/mod.rs` | `SearchVault`, `HitActivated`, `VaultSearchDelegate` |
| `crates/app/src/vault_search/search.rs` | background: lookup, `search` per chunk, merge |
| `crates/app/src/vault_search/rows.rs` | `Row`, `Results::new(searched, path_of)` |
| `crates/app/src/vault_search/render.rs` | header and hit elements |
| `crates/app/src/vault_search/tests.rs` (+ `tests/` if >400 lines) | delegate tests |
| `crates/app/src/picker/mod.rs` | `empty_text` → `Option<SharedString>`; `indexing_footer` |
| `crates/app/src/picker/runs.rs` | `SEARCH_DEBOUNCE` moves here from quick open |
| `crates/app/src/picker/letters.rs`, `quick_open/{mod,tests}.rs`, `quick_open/tests/rows.rs`, `shell/tests/mod.rs` | follow the two moves |
| `crates/app/src/shell.rs` | `search_vault` action handler |
| `crates/app/src/keymap.rs` | `cmd-shift-f` → `SearchVault`, `AppShell` |
| `crates/app/src/lib.rs` | `mod vault_search` |
| `crates/app/src/theme.rs` | `UI_DIGIT_WIDTH` |
| `crates/app/src/shell/tests/vault_search.rs`, `shell/tests/mod.rs` | shell tests |
| `crates/app/src/shot/scenes.rs`, `shot/scenes/vault_search.rs` | six scenes |
| the integration spec | "As built (slice 3)" |

## Data shapes

```rust
// vault_search/mod.rs
actions!(vault_search, [SearchVault]);

/// Slice 4 selects `range`; this slice opens the note at its top.
pub(crate) struct HitActivated { pub(crate) note: EntryId, pub(crate) range: Range<ByteOffset> }

pub(crate) struct VaultSearchDelegate {
    links: Entity<IndexSync>,
    vault: Entity<Vault>,          // the header's path, read on landing
    results: Results,
    /// Stored on landing: "No matches in …" reads the progress of that landing.
    empty: Option<SharedString>,
    runs: Runs,
    #[cfg(test)] hold: Option<Shared<oneshot::Receiver<()>>>,
    _subscriptions: Vec<Subscription>, // Runs::observe only; no NotesChanged
}

// vault_search/rows.rs
pub(super) enum Row { Header(Header), Hit(HitRow) }
pub(super) struct Header { name, folder, count: SharedString, label: SharedString }
pub(super) struct HitRow {
    note: EntryId, range: Range<ByteOffset>,
    line: SharedString, context: SharedString, lit: Range<usize>, label: SharedString,
}
pub(super) struct Results { rows: Vec<Row>, digits: u8, limit_reached: bool }
```

## Behaviour

- `update_matches(query)`: `runs.start`. Before the first build → empty
  text "Scanning the vault…", no scan. Raw query empty → no rows, no text,
  no scan. `NoteQuery::new(query, None, &[])` with no term → "No matches",
  no scan. Else `runs.scanning`, wait `SEARCH_DEBOUNCE` only when
  `Run::Search` is `behind_user`; `Run::Refresh` and `Run::Notify` wait
  nothing (never `unreachable!`). Then `search::searched` → `apply`.
- `search::searched(executor, snapshot, query, cancel) -> Option<Searched>`:
  `lookup` spawned, `search` per `snapshot.chunks(executor.num_cpus())`
  spawned, `Searched::merge(_, LIMIT)` spawned; `None` when cancelled.
- `apply`: drops a stale id. Reads the kept key `(note, range.start)` of the
  selected hit for a background run. Builds `Results` with the vault's entries:
  a note with no entry is dropped with its hits. Stores `empty`. Selects the
  kept row through `matches_refreshed`, else row 0 through
  `matches_updated`. Then `Runs::landed`.
- `confirm(ix)`: `runs.confirmed()`; a hit row emits `HitActivated`.
- Footer, read live: `limit_reached` → "1,000+ results; refine the query";
  else building with rows → the quick-open indexing footer, shared.
- Shell: `search_vault` mirrors `quick_open` (no vault → no-op,
  `toggle_picker`, `HitActivated` → `show_note(note)`, subscription held by
  the modal).

**Why `HitActivated` now:** slice 4 then only changes the shell's handler
(`show_note` → `show_hit`); the delegate, its event and its tests stay. With
`NoteActivated`, slice 4 would change the event, the delegate's `confirm`,
its tests and the shell subscription.

## Rendering

All rows one `UI_LINE_HEIGHT` (the picker's `row_height`). Header: name,
folder (dimmed, `flex_1`, cut from the start), count (dimmed, `flex_none`).
Hit: line number dimmed, right-aligned in a box `digits × UI_DIGIT_WIDTH`
wide (Zed's `ilog10 + 1` × 0.5 rem), gap, then the context with
`match_text` on its match, end ellipsis. Everything through `one_line`.

## Steps

1. Trait: `empty_text` returns `Option<SharedString>`; `SEARCH_DEBOUNCE` to
   `runs.rs`; `indexing_footer` to `picker`. Quick-open tests unchanged in
   name (`./scripts/test-names.sh` before/after).
2. Write the delegate tests and the shell tests; see them fail.
3. `search.rs`, `rows.rs`, `mod.rs`, `render.rs`; shell, keymap.
4. Tests green; clippy; fmt per crate.
5. Six scenes; render; look; fix.
6. Spec "As built (slice 3)".

## Verification

```
./scripts/test-names.sh > /tmp/before.txt   # on db88994
./scripts/check.sh
./scripts/test-names.sh > /tmp/after.txt; diff /tmp/before.txt /tmp/after.txt  # additions only
cargo run -p inkra-app --features shot --example shot vault-search-results   # and the five others
```

## Red/blue on the plan

| # | Red claim | Blue | Verdict |
|---|---|---|---|
| 1 | No `NotesChanged`: a trash while loads land lingers up to `BUILDING_REFRESH` | Confirmed, by design (S1, S3) | Keep; recorded in "As built" |
| 2 | The refresh relies on `apply_change`'s notify, unpinned | Refuted: the scanning test pins it | Keep the test |
| 3 | Each chunk gets the full `LIMIT`: up to `num_cpus × 1,000` contexts before the merge | Confirmed; a chunk cannot take less and keep "merge equals one range" | Known cost, background only; recorded |
| 4 | `Rows::new` on the foreground is unmeasured | Confirmed gap | Measure 1,100 rows in release once; record the figure |
| 5 | `HitActivated.range` unused | Confirmed, per the Slices section | Keep |
| — | A cancelled run leaves `scan` set | Refuted: `Runs::start` takes it on every run | — |

## Red/blue on the diff

| # | Red claim | Blue | Verdict |
|---|---|---|---|
| 1 | No test: a user run could keep a moved selection | Confirmed | `a_new_query_selects_its_first_hit`; fails with the `!user` filter removed |
| 2 | The plan names `Rows { digits: usize }` | Confirmed on the staged plan | Plan updated to `Results { digits: u8 }` (`Rows.rows` fails `struct_field_names`) |
| 3 | `footer` names `gpui::App` in full | Confirmed | `App` imported |
| — | `enter` on a field with no term; a header with no hit; a click on a header | Refuted: no-op, `NoteHits` never empty, `is_selectable` gates the click | — |

## Review follow-ups (a6134ee, ef41d26), red/blue on their diff

| # | Red claim | Blue | Verdict |
|---|---|---|---|
| 1 | `Runs::scanning`'s future can be dropped silently | Refuted: `Future` is `#[must_use]`; a /tmp crate warns, `-D warnings` fails | — |
| 2 | The spec still names `search.rs` | Confirmed (S3, two places) | S3 now points at `index::fan_out` |
| 3 | `Bottom` ignores the reveal offset | Refuted as a bug: Bottom shows the rows above the selection; one header per group | — |
| 4 | The reveal lead is unbounded | Harmless: gpui clamps; one header per group | — |

# Feature registry: one base for built-in and external features

Status: **implemented**, slices 0 to 2 merged 2026-11-13 as PR #41 (`6a932d54`).
Revision 4 closes O1 and O5 and aligns D3 and D6 with the code.
Revision 1 was attacked by three reviews, revision 2 by a second red team (see
Review). Revision 3 applied their fixes and four owner verdicts: decorations
leave this spec, the host surface is built now, switches are per vault, and
built-ins reach the index through a floor-2 accessor. Read D1, D3, D4 and D9 first.

## Context

The owner wants features on a shared base. Four properties are required:

- add as many features as wanted;
- no effect on launch time;
- every feature can be switched on or off; a core item (D2) is not a feature;
- users can add their own, and those can have their own panes and change how
  the editor renders text.

Inkra has no such base today. Every feature is wired by hand:

| Seam | Today | Ref |
|---|---|---|
| Dock views | closed enum `View { Files, Backlinks }`; `View::ALL` iterated by the workspace invariant and the session restore | `crates/layout/src/lib.rs:14-45,215-231`, `shell/workspace.rs:114-117` |
| View bodies and headers | one `match` per view in the shell | `shell/dock_chrome.rs:135-180` |
| Key bindings | one `Vec<KeyBinding>`, bound once at launch | `crates/app/src/keymap.rs`, `bootstrap.rs:265` |
| Palette | lists the dispatch path's rendered `on_action` listeners; hide list is an array checked with `.contains` | `command_palette/commands.rs`, gpui `key_dispatch.rs:363-379` |
| Quick open | an `on_action` handler on the shell reading `panes.links`, `showing`, `recent` | `shell.rs:297-308`, `shell/render.rs:79` |
| Backlinks | built eagerly with the vault; called directly on activation; rebuilds on every index change, shown or not | `shell/panes.rs:38-47`, `shell/activation.rs:226`, `backlinks/mod.rs:66-90` |
| Settings | `.inkra/settings.json` read by the theme store; hot reload through `ConfigChanged` | `theme/store.rs:152`, `vault/events.rs:74`, `shell.rs:175` |
| Registry | none; `lib.rs` is a flat module list | `crates/app/src/lib.rs` |

Two audits back this spec, both in `.superpowers/explorations/2026-11-12-extensions/`
(gitignored): `audit-impact.md` (what the design changes, file by file) and
`audit-neutral.md` (an independent opinion, written without our choices).

The four engine demos (`.superpowers/explorations/2026-11-12-engine/`) are engine
features. This spec does not build them. They feed the list of future hooks (D12).

## Decisions taken in the brainstorm

These are settled. A change goes back to the owner.

- **B1. One feature API**, used by Inkra's own features too. It covers what a
  feature declares (its manifest) and what it asks of the host (D4). Zed's
  built-ins do not use its extension API; this is a deliberate deviation.
  Owner verdicts 2026-11-13: the host surface is built now, not later; built-in
  features reach the index and the vault through floor-2 entity accessors, and the
  index's value API is designed in the WASM spec against a real guest (D4).
- **B2. Two deliveries of one Rust crate**: compiled into the binary, or compiled to
  WASM (`wasm32-wasip2`) behind a versioned WIT. Extensions are Rust only.
- **B3. No native dylibs.** Rust has no stable ABI; Bevy deprecated dynamic plugins
  in 0.14 and removed them in 0.15.
- **B4. Two tiers sharing one switch**, as Obsidian's core and community plugins:
  built-in features (on by default) and external features (off by default).
- **B5. Two floors.** Floor 1 serves both deliveries: data contributions and a
  declarative UI (a component tree the host renders with `app/src/ui/`, the Raycast
  model). Floor 2 is native only: direct gpui access.
- **B6. Every feature computation is measured against a budget** (D11).

## Decisions of this spec

### D1. Scope

In scope:

1. the registry: manifest, `FeatureCx`, triggers, handle (D3, D6);
2. the host surface a feature calls (D4) and the pane contract (D5);
3. per-vault switches and the settings module (D7);
4. commands and keys gated by the key context (D8);
5. the disable transaction (D9);
6. dock views by id (D10);
7. quick open and backlinks moved onto the registry; Files moved onto the pane
   contract; a fake feature in tests.

Out of scope, each for a later spec:

- **editor decorations.** Owner verdict 2026-11-13. The three reviews found the
  revision-1 seam unbuildable: `Version` is not ordered, markdown structure cannot
  be shifted through an edit, and it contradicted the highlighting spec. It becomes
  a revision of `2026-11-11-code-highlighting-design.md`: the `edit_mark` log, then
  additive style layers with highlighting and a test fake as the two implementers;
- the WASM tier: runtime, WIT, capabilities, the precompiled cache;
- the declarative UI host (floor 1's component set);
- the line and block fold layer, and Télescope on top of it;
- the user tier's language (Rust/WASM against Luau, O4).

Why these implementers: CLAUDE.md allows an abstraction only at a real seam. Quick
open (a command and a modal) and backlinks (a pane) are the registry's two real
features; the fake feature exercises the edges neither reaches (a conflicting key,
a pane holding an input field). Files and backlinks are the pane contract's two
implementers.

### D2. What has a switch

A feature has a switch when Inkra stays a complete note app without it and nothing
else depends on it. Otherwise it is core.

| Core, no switch | Built-in, switch, on by default |
|---|---|
| editor, live preview, code blocks | backlinks |
| vault, `Fs`, watcher | quick open (`⌘P`) |
| file tree | vault search (`⌘⇧F`), after this spec |
| tabs, splits, docks | code highlighting, when it lands |
| index and link resolution | |
| following a link | |
| settings, themes, command palette | |

Each core item fails the test on the code. The file tree is the only surface for
new folder, rename and trash (`keymap.rs:71,74`). The palette is the only door to a
command with no key (`SelectTheme`, `RebuildIndex`). Following a link resolves
through the index (`activation.rs:246-257`). A core item uses a registry contract
where one exists (Files implements `Pane`) and has no manifest. The tree gets a
switch when its operations have a second surface; the palette when a second
surface lists every command.

Prior art agrees. Neovim's `gx` lived in netrw, so switching netrw off broke it;
0.10 moved `gx` into core. Zed always loads its project panel and switches only
its AI panel (`zed.rs:778-855`). Obsidian can switch its file explorer and palette
off because a menu and the Hotkeys page offer the same operations; Inkra has
neither.

### D3. A feature is a const manifest plus an activation function

```rust
// sketch, names not final
pub(crate) struct Manifest {
    id: FeatureId,                    // "backlinks"; also its key context identifier
    name: &'static str,
    default_on: bool,
    commands: &'static [CommandDecl], // action name, label, default keys, hidden
    panes: &'static [PaneDecl],       // ViewId, title, icon, default dock
    triggers: &'static [Trigger],     // PaneShown, CommandRun, AfterFirstFrame
    activate: fn(&mut FeatureCx) -> FeatureHandle,
}
```

`CommandDecl`, not `Command`: `command_palette::commands::Command` already exists.

`activate` is a function pointer in the manifest, so the registry stores
`&'static Manifest` values and needs no trait object.

A feature's state lives in its own `Entity`, created in `activate` and owned by the
handle. It never lives in the registry: a handler then needs the shell, its
context, the `Window` and the feature state at once, and the borrow checker
refuses that. `activate` registers one boxed handler per command on the handle:

```rust
// sketch, names not final
pub(crate) struct FeatureHandle {
    handlers: Vec<(TypeId, Box<dyn FnMut(&dyn Action, &mut FeatureCx)>)>,
    _owned: Vec<AnyEntity>, _subscriptions: Vec<Subscription>, _tasks: Vec<Task<()>>,
}
```

An action adapter (D8) takes the handle out of the registry, calls the handler,
and puts the handle back. The handler reaches its state through the entity it
captured. `FeatureCx` holds an `Option<&mut Window>`: an action brings a window, an
event subscription does not.

As built (`feature/mod.rs`, `feature/registry.rs`, `shell/features.rs`): `activate`
is `fn(&mut FeatureCx)` and returns nothing; the registry creates the handle and
`FeatureCx` fills it. The manifest has no `name`, and `after_first_frame: bool`
replaces `triggers` (D6). Handlers are `Rc<dyn Fn(&dyn Action, &mut FeatureCx)>`.
The handle holds handlers, entities and subscriptions, and no tasks: a feature's
tasks live in its own entity and end with it. `FeatureCx` holds a `&mut Window`,
not an `Option`, since every activation and handler runs with a window.

The registry is owned by the
`AppShell`, one per shell: `⌘O` builds a fresh shell by `replace_root`
(`shell/open_vault.rs:125`), and a feature holds vault-specific entities. Zed does
the same: its features register per workspace through `observe_new::<Workspace>`.
The wiring lives in `shell/features.rs`; `shell.rs` is at 388 lines.

`FeatureCx` is the only door a feature gets. It wraps the shell's context and
exposes only what the handle tracks: host calls (D4), subscriptions to host events,
owned entities, owned modals, pane registration (D5). It does not
hide gpui: `FeatureCx::new` and `update` hand the feature its entity's `Context`,
which derefs to `App` (`shell/features.rs:57-72`). The ast-grep rule below is the
real guard. Dropping the `FeatureHandle` drops every subscription and entity, and
with them the tasks those entities hold, since a dropped gpui `Task` is cancelled.

An ast-grep rule in `.ast-grep/rules/`, scoped by path to the feature modules
(`backlinks/`, `quick_open/`, `feature/`), bans the registrations that outlive
every entity: `cx.on_action(`, `set_global(`, `bind_keys(`, `observe_global(`,
`observe_new(`, `observe_keystrokes(`. `App::on_action` returns no `Subscription`
(gpui `app.rs:2750-2763`), so a feature that called it could never be switched off.
The rule is syntactic: it matches a receiver named `cx`, so a renamed context slips
through, and the element builder `.on_action` on a `div` stays allowed. Review
covers the gap. Subscriptions taken in the feature entity's own `Context` die with
that entity and need no ban. A detached task must end when its entity is released,
as quick open's excerpt reads do (`quick_open/list.rs:175`, the loop breaks when
`picker.update` fails). `check.sh` already runs the rules.

The registry lives in `crates/app/src/feature/`. It moves to a gpui-free crate when
the WASM host needs to read manifests without gpui. Built-in features stay inside
`app`, so views stay `pub(crate)` and the no-gpui-below-app rule holds.

### D4. The host surface

A feature reaches the shell, the index and the vault only through `FeatureCx`.
The surface has two parts.

**Shell calls, floor 1.** They take and return plain values, so the WIT can mirror
them later:

| Call | Returns or does | Replaces |
|---|---|---|
| `shown()` | the note shown: a note, an image, or nothing | the `set_file` push from `activation.rs:226` |
| `NoteShown` event | the same value, on every change | |
| `recent()` | the recent-files order | `shell.recent` |
| `open(file, Open)` | opens a file in the place `Open` says | `shell.open(...)`; backlink rows' `FileActivated` |

A feature reads `shown()` when it activates, then subscribes to `NoteShown`, so a
feature activated late starts from the current state. Revision 1 relied on the
event alone, and a panel shown after the note opened stayed empty.

**Index and modal access, floor 2.** Owner verdict 2026-11-13:

| Call | Returns | Used by |
|---|---|---|
| `index()` | `Entity<IndexSync>` | quick open's `IndexDelegate` and `Runs`; backlinks' `backlinks`, `cached`, `reader`, `forget`, `stale` |
| `vault()` | `Entity<Vault>` | display names |
| `open_picker(delegate)` | a modal owned by the handle; closed on disable | `toggle_picker_with` |

Quick open and backlinks lean on the whole index machinery: `Runs` and its
`Ledger` (the deferred re-run), excerpts streamed through `reader(cx)`
(`quick_open/list.rs:145`), and backlinks writing back to the index
(`backlinks/job.rs:114-121`). A value API in front of that would be a rewrite of a
tuned path for a tier not yet designed. The WASM spec designs the index's value
API against a real guest. Floor-2 features may also import the row helpers
`row_text`, `LitText` and `ui::ListItem`.

With no vault open, no feature activates and no adapter renders: today quick open
already returns early without panes (`shell.rs:298`).

Revision 2 listed `follow(spot)`. It is dropped: `Spot` and `LinkActivated` come
from the link-landing spec, which is not implemented. The call joins this table
when that spec lands.

Deviation (slice 1b): `index()` and `vault()` return an `Option`, `None` with no vault
open, so a feature's `activate` returns early instead of the host checking first. Quick
open's most-recent order is the shell's `recent`, read through `recent()`; the feature
keeps no list of its own.

`FeatureCx` is a concrete struct with one implementation. Tests drive it through a
real `AppShell` on `MemFs`, as every app test does, so it needs no trait.

### D5. The pane contract

```rust
// sketch, names not final
pub(crate) trait Pane: Render {
    fn header(&self, cx: &App) -> PaneHeader;
    fn focus(&self, _cx: &App) -> Option<FocusHandle> { None }
    fn hidden(&mut self, _cx: &mut Context<Self>) {}
}
pub(crate) struct PaneHeader {
    count: Option<usize>,
    buttons: Vec<HeaderButton>, // icon, tooltip, action to dispatch
}
// what the shell stores: the trait is not dyn-compatible, since `Render::render`
// returns `impl IntoElement`
pub(crate) struct PaneEntry {
    view: AnyView,
    header: Box<dyn Fn(&App) -> PaneHeader>,
    focus: Box<dyn Fn(&App) -> Option<FocusHandle>>,
    hidden: Box<dyn Fn(&mut App)>,
}
```

`FeatureCx::register_pane::<T: Pane>(entity)` builds the `PaneEntry`, as Zed's
`PanelHandle` wraps a `Panel`. The shell renders every pane through it: body,
header, focus. The `match view` arms in `render_view_body` and `render_view_header`
(`dock_chrome.rs:135-180`) go away. Files implements `Pane` as a core pane;
backlinks as a feature pane. That makes two implementers, plus a fake pane with an
input field in tests. This follows Zed's `Panel` trait (`workspace/src/dock.rs:36`),
reduced to what a note app's docks use.

Focus is optional: backlinks has no `FocusHandle` today, and this spec does not
give it one. `hidden` replaces Files' special case in `change_layout`
(`shell/workspace.rs:54-69`): when a pane stops being visible, the shell lands
focus if the pane held it, then calls `hidden`. Files cancels its name edit there.
The fake pane's test checks that its input loses keyboard focus when hidden. Pane
navigation's special case for Files (`shell/split.rs:113`) moves behind `focus`.

Header buttons dispatch actions; they no longer call shell methods through
`cx.listener` (`dock_chrome.rs:144-179`). `NewNote` and `NewFolder` are core
actions. `SearchVault` becomes a feature action, so its button renders only while
`window.is_action_available` says so.

As built, `PaneHeader` is an enum: `Title { count }` for a titled header, or
`Buttons(&'static [HeaderButton])` for a bar of buttons, as Files draws today. The
`SearchVault` gate is deferred. `SearchVault` is still a core action, so the button
always renders; the gate comes with the slice that moves vault search onto the
registry. `Pane` also has `shown`, the pair of `hidden`, which the shell calls
when a registered pane starts showing.

`PaneDecl` also carries the pane's move action, which `open_dock_menu` builds by
`match view` today (`shell/docks.rs:99-102`), and its debug selector
(`dock-icon-<id>`, which tests already use). The move action activates nothing: it
moves the pane, and the pane activates when shown.

Panes are singletons: a `ViewId` names both the pane type and its one instance. A
second instance of one pane is not on the roadmap.

### D6. Activation and the launch

The property: **a feature adds no main-thread time before the first frame, and does
no work while hidden.** `Timeline` gets one mark for the registry and one per
activation, so a regression shows in `scripts/bench-compare.sh`.

Revision 1 asserted "no feature entity before the first trigger". That target was
wrong. Both docks show by default (`crates/layout/src/lib.rs:94-104`), so backlinks
would activate on the first frame anyway. Its real cost is `rebuild()` on every
index change while hidden (`backlinks/mod.rs:73-74`).

Triggers:

- `PaneShown`: the pane becomes visible. A pane visible at launch activates from a
  `window.defer` queued by the shell's first render, so it runs after that frame is
  drawn; for one frame the pane draws its empty state. Not `on_next_frame`: its
  callbacks run only inside the platform frame callback (gpui `window.rs:2008-2023`),
  and `TestWindow` never calls it unless a test simulates a frame request, so every
  `#[gpui::test]` would see an inert pane. The shot stage must run the defer before
  it captures, or the scenes with backlinks rows (`shot/scenes.rs:189,243-245,637`)
  change; slice 2 checks it.
- `CommandRun`: the first run of one of its commands (D8).
- `AfterFirstFrame`: VS Code's `onStartupFinished`, for features with no pane or
  command to wait for.

Deviation (slice 1a): `AfterFirstFrame` is `Manifest.after_first_frame: bool`, not a
`triggers` list, and it activates from a task spawned in `AppShell::new` with
`cx.spawn_in`, not from a `window.defer` queued by the first render. `open_window`
draws once before it returns, and a foreground task cannot run before that turn ends.
The property becomes: no activation inside `AppShell::new` or inside the first draw;
activation runs after `open_window`'s first draw, possibly before AppKit presents it
(`docs/startup-floor.md:63-65`: AppKit shows the window about 23 ms after the first draw). A
`window.defer` has the same limit, so this loses nothing. Truly after the present
needs `window.on_next_frame` and then a spawn, which costs a `simulate_next_frame` in
every window-opening test helper and in the shot stage's settle; that is an owner
question. Tests drain the task with `run_until_parked`; the shot stage drains it in
`settle`. A feature switched on after the launch activates in the turn its settings
reload lands. `Timeline` marks `registry` when the task runs and `activated` per
feature; the launch line prints once, so an activation after it is not in it.

A feature that owns a pane stops work when the pane is hidden. Backlinks marks
itself stale on `NotesChanged` and on a build applying while hidden, and rebuilds once when shown.

Activation states, as built: off, dormant, or active (`Slot`, `feature/registry.rs`).
`activate` is infallible, so there is no failed state; a feature that cannot start
(no vault open, D4) returns early and stays inert.

### D7. The switch, per vault

Owner verdict 2026-11-13: switches are per vault.

`.inkra/settings.json` gets one key:

```json
{ "features": { "backlinks": false } }
```

A missing entry means the manifest's `default_on`.

A new `app/src/settings.rs` reads `settings.json` once per vault adoption and
hands the result to both the themes and the registry. The read moves out of the
theme store (`theme/store.rs:151-153`), which parses only `theme` and
`code_block_wrap` today; `features` joins them. `write_choice` already keeps keys
it does not know (`theme/store.rs:249-268`). The resolved settings are already
loaded before the shell is built, at launch (`bootstrap.rs:284-289`) and on `⌘O`
(`open_vault.rs:146`). The switches ride along, and reach the shell **before** it
restores views or arms triggers:

- at launch, as an argument of `AppShell::new`;
- on `⌘O`, as an argument of the new shell, not after it (`open_vault.rs:125-129`);
- on `ConfigChanged` (`shell.rs:175`), as a diff applied on the turn its async
  reload lands: off runs D9, on registers and re-arms.

A shell never builds a disabled feature, even for one frame.

### D8. Commands and keys

Keys: `key_bindings()` lists every built-in feature's bindings once, at launch, in
the context `AppShell && <feature id>`. The shell root's key context adds one
identifier per enabled feature. Zed sets runtime identifiers the same way
(`workspace/src/workspace.rs:8354-8376`, `debugger_running`). A disabled feature's
keys stop matching in that shell, and the keymap is never rebuilt. Two windows on
two vaults then need no second keymap. Revision 1's `clear_key_bindings` rebuild is
dropped. Binding before the settings load (`bootstrap.rs:265`) becomes harmless.

Verified in gpui: an `And` predicate tests both identifiers on the same context
(`keymap/context.rs:275-322`), `_` is legal in an identifier (`context.rs:498-499`),
and `bindings_for_action` respects the context (`key_dispatch.rs:401-433`), so the
palette's key hints follow the switch.

Dispatch and the palette: the palette lists the listeners rendered on the dispatch
path, not the bindings. `shell/features.rs` therefore renders one `on_action`
adapter per command of each **enabled** feature. The first run activates the
feature; the adapter then calls the handle's handler (D3). A disabled feature renders no adapter, so it
has no palette entry and no dispatch. Listeners are rebuilt every frame, so the
switch takes effect on the next frame. A palette already open dispatches a stale
entry into nothing.

Actions stay real gpui types, owned by their feature. `MoveBacklinksToOtherDock`
moves into the backlinks feature (`shell/docks.rs:9-17,69-76,100-101`). Each pane
keeps its own action type: a single action parameterized by `ViewId` would fail
gpui's `available_actions`, which builds actions by type with default data and
skips the rest. Core keeps its hide array; a feature command carries `hidden`.

Tests: a cross-feature test that no two commands share a key in one context, with
the list checked before deduplication. `simulate_keystrokes` for `⌘P` with quick
open on, off, and on again. The existing `no_key_is_bound_both_shared_and_per_host`
(`keymap/tests.rs`) stays.

### D9. The disable transaction

Dropping the handle is the last step, not the whole of it. Three facts make that
necessary:

- the modal holds a strong clone of the picker (`shell/modal.rs:62`);
- the layout, inactive tabs' parked slots and the closed stack keep the pane's
  `ViewId`;
- `tab_of` panics when a view is missing (`shell/workspace.rs:18-26`), and
  `reveal`, `is_visible` and `side_of` call it.

Switching a feature off runs, in order:

1. mark it disabled: its key context identifier and its action adapters are gone
   from the next frame (D8);
2. close its modal if open (`dismiss_modal` drops the strong clone and restores
   the saved focus, `shell/modal.rs:145-157`);
3. remove its panes in one `change_layout`, before anything else can call
   `tab_of`: choose the surviving active view in each group, drop a group left
   empty, close a centre `Shown::View` tab, land focus if a removed pane held it;
4. drop its `Place::View` entries from inactive tabs' `Parked::View` slots
   (`crates/tabs/src/parked.rs:18`) and from the closed stack
   (`crates/tabs/src/closed.rs:28-31,120-128`), through a predicate API added to
   `inkra-tabs`. The back and forward history never holds a view:
   `Stack::push` skips a place with no file (`crates/tabs/src/history.rs:59-60`);
5. drop the handle: subscriptions end, tasks are cancelled, entities are released;
6. save the session.

`tab_of` returns an `Option`, and so do its three callers. The workspace invariant
and the restore defaults iterate the enabled catalog, not `View::ALL`. Switching a
feature back on puts its panes in their manifest's default dock, last, inactive.

As built, step 3 calls `detach` on each of the feature's tabs, and step 4 needs no
`inkra-tabs` API. Four facts make this enough:

- `close_tab` is wrong here: it sends a view tab to its dock before `tabs.close`
  runs, so the pane would survive in its dock;
- `remember` skips a blank entry, so the closed stack never holds a view;
- an inactive `Parked::View` slot is the pane's own tab, which `detach` removes;
- `Tabs::take`, which `detach` calls, leaves the closed stack alone.

`Parked` lives at `crates/app/src/shell/parked.rs:18`, not
`crates/tabs/src/parked.rs:18` as step 4 says. Switching a feature back on appends
each pane last and inactive to its side's first dock group, or to one new dock
group. The dock stays shown or hidden as it was.

Tests, each a `#[gpui::test]` on `MemFs`:

- disable quick open with its picker open and a query outstanding: no modal, focus
  restored, the query's late result goes nowhere;
- disable backlinks while it is focused and the only tab of its group;
- close a tab showing backlinks, disable backlinks, then `⌘⇧T`: nothing reopens a
  disabled pane;
- restart with backlinks disabled: the session restores without it, no panic;
- re-enable: the pane returns to its default dock;
- after disable, `run_until_parked`, then the feature's `WeakEntity` no longer
  upgrades.

### D10. Dock views by id

`View { Files, Backlinks }` becomes `ViewId(&'static str)`. `inkra-layout` stays
gpui-free: it stores ids and never asks what a view draws. Layout functions that
iterated `View::ALL` or called `default_side` take the catalog as an argument.

The session needs no format change. It is already name-based (v3,
`StoredDock.views: Vec<Option<String>>`, `crates/layout/src/lib.rs:204-207`), and
restore already drops a name it does not know. A disabled feature's pane is
dropped the same way.

This is slice 0: about 151 sites in 31 files, 12 of them shell test files, no
behaviour change.

### D11. The budget

Native code cannot be preempted, so for built-ins the budget is measured, not
enforced. Benches are "before/after comparison, not a gate" (CLAUDE.md), so the
gates are deterministic count tests, in the style of `shape_count`:

- launch with every feature on: no feature activates before the first frame;
- hidden backlinks: zero rebuilds across `NotesChanged` and build applies; one rebuild when shown;
- a feature adds no work to the keystroke path: no feature hears a keystroke
  event in this spec.

Timing is for diagnosis: each activation and each feature event handler is timed,
and one over 2 ms logs a line with the feature id. The `Timeline` marks of D6 show
in `bench-compare.sh`.

The keystroke budget for decorations (one shared foreground budget per keystroke)
moves to the decorations spec. Auto-disable after repeated overruns, and preemption
by epoch deadline, belong to the external tier.

### D12. Known future hooks

From the engine demos. Listed so D3 and D4 leave room; none is built here.

| Hook | Needed by | Blocked on |
|---|---|---|
| decoration layer (styles, conceals) | highlighting, then external features | the `edit_mark` log; the highlighting spec revision |
| fold-range producer (hide whole lines or blocks) | Télescope | the decoration layer, then the fold layer |
| line breaker | Knuth–Plass (typographie) | a seam at `editor/layout/shape.rs:56` |
| transaction observer | Genèse | the `edit_mark` log, exposed as an event |
| list-row hover preview | Feuilleter | a hover contribution on `ui::ListItem` |
| floor-2 inline element (a widget in place of a range) | formulas, diagrams, task boxes | the decoration layer plus a layout slot |

### D13. Constraints that keep the WASM tier possible

- Manifests are plain data with no gpui type.
- Shell calls (D4) take and return values; events carry ids and values.
- A feature never holds a host borrow across a call.
- Floor-2 calls (`index()`, `vault()`, `open_picker`, `Pane`) are the ones the WIT
  spec must replace: the index by a value API with revisions and cancellation, the
  picker and panes by declarative equivalents.

### D14. Not built

Native dylibs, an out-of-process host, webviews, a hook-everything event bus, a
stable API promise, a marketplace, multiple instances of one pane.

## What Zed does, and where we deviate

- Zed reads one `index.json` at startup and registers contributions from it as data
  (`vendor/zed-reference/crates/extension_host/src/extension_host.rs:416-453`).
  Built-in manifests here are `const`, so there is no file at all.
- Zed registers features per workspace (`observe_new::<Workspace>`). We follow it:
  one registry per `AppShell`.
- Zed loads its panels from a task spawned while the workspace is set up
  (`vendor/zed-reference/crates/zed/src/zed.rs:778-818`). We follow it for
  `AfterFirstFrame` (D6), in place of a `window.defer` from the first render.
- Zed adds runtime identifiers to the workspace key context. We follow it for the
  switches (D8).
- Zed's docks render through the `Panel` trait. We follow it with a smaller `Pane`
  (D5): no zoom, no position menu, no multiple instances.
- Zed compiles every WASM extension at each launch, cache in RAM only
  (`wasm_host.rs:1003-1022`). The WASM spec will instantiate on first use from a
  disk cache.
- Zed has no disabled state: disable means uninstall (`extension_host.rs:198-204`).
  We have a switch and a disable transaction.
- Zed's built-ins do not use the extension API; only the sinks are shared
  (`crates/extension/src/extension_host_proxy.rs:26-35`). We dogfood (B1).
- Zed's extensions draw no UI. We plan a declarative UI (B5), in a later spec.

## Slices

Each slice is vertical: data, behaviour and test, and adds its own `Timeline` marks
and count tests.

0. **`ViewId`.** `View` becomes `ViewId(&'static str)`; layout takes the catalog;
   `tab_of` returns an `Option`. No behaviour change. `./scripts/test-names.sh`
   before and after: no test lost.
1a. **Registry and switches.** `feature/`, `Manifest`, `FeatureCx`,
   `FeatureHandle` with its handlers, the `settings.rs` module, per-vault switches
   threaded through `AppShell::new` and the `⌘O` build (every test harness call
   site), key context gating, rendered action adapters, the `CommandRun` and
   `AfterFirstFrame` triggers, the ast-grep rule, and one fake feature with a
   command and a conflicting key. Tests: the fake command on, off, on again by
   `simulate_keystrokes`; its palette entry gone when off; vault A then vault B with
   opposite switches, B never builds a disabled feature; the cross-feature key test.
1b. **Quick open on the registry.** Its state in its own entity, `QuickOpen` owned
   by the feature, `recent`, `open`, `index()`, `open_picker`, the disable
   transaction's modal steps. Tests: `⌘P` on, off, on again; disable with the picker
   open and a query outstanding.
2. **Pane contract and backlinks.** `Pane`, `PaneEntry`, `PaneHeader`, Files and
   backlinks through them, header buttons as actions, the `PaneShown` trigger by
   `window.defer`, `shown()` and `NoteShown` replacing `activation.rs:226`, `vault()`,
   no work while hidden, the disable transaction's pane steps and the `inkra-tabs`
   predicate API, a fake pane with an input field. Tests: D9's list; the hidden-dock
   case (hide the right dock, open a note, show the dock, the rows are right); zero
   rebuilds while hidden; the fake pane's input loses focus when hidden; the shots
   unchanged. `inkra-tabs` changes, so `cargo mutants --in-diff` runs on it.

## Open questions

- **O4.** User tier: Rust compiled to WASM, or an embedded script runtime (Luau)
  for save-and-reload? The neutral audit found no trustworthy numbers; a short
  spike of each would settle it. Next spec.

O2 (highlighting as the second decoration producer) left with D9 of revision 1.
O3 (switch scope) is settled: per vault.
O1 (the core list) is settled in revision 4: as in D2, file tree included.

O5 (floor 2) is settled in revision 4. Floor 2 is not a contract. Built-in
features live in `app`, `pub(crate)`, and use gpui through `FeatureCx` and their
own entities' `Context`. It follows gpui's pinned rev, and gpui is pre-1.0, so it
promises nothing (D14). A floor-2 call becomes public when a compiled-in feature
outside `app` needs it. That change cuts a feature crate and passes the catalog
from `main`. `Registry::new` already takes the catalog; outside tests, only
`bootstrap.rs:297`, `keymap.rs:31` and `Registry`'s `Default` name `BUILTIN` today. The
WIT mirrors floor 1 only (D13).

## Review

Revision 1 was reviewed on 2026-11-12/13 by three reviewers working separately:
an opus red team against the code, the advisor (opus) on prior art and failure
modes, and Codex (gpt-6-astra). Reports:
`.superpowers/explorations/2026-11-12-extensions/review-codex.md`; the two opus
reports were returned in session. All three said "revise before implementation".

Where they agreed, and what this revision does:

| Finding | Revision 2 |
|---|---|
| The decoration seam is unbuildable (unordered `Version`, markdown structure, no merge contract, no layout revision, conflicts with the highlighting spec) | out of scope (D1); goes to a highlighting spec revision |
| The registry must be per shell; settings load in the wrong order | D3, D7 |
| Dropping the handle revokes nothing (modal, layout, history, `on_action`) | `FeatureCx`, ast-grep rule (D3); disable transaction (D9) |
| The palette lists rendered handlers, not bindings; a parameterized action vanishes | rendered adapters, one action type per pane (D8) |
| Lazy activation saves nothing for backlinks; a late panel misses `NoteShown` | the D6 property; `shown()` snapshot plus event (D4) |
| The budget gates nothing | count tests as gates, timing as diagnosis (D11) |
| Features need a host surface and a pane contract | D4, D5 (owner verdict on B1) |
| Slicing found the hardest parts last | slice 0; quick open first with a fake feature |

Where they diverged:

- **Decorations.** The red team wanted them split out; the advisor wanted a complete
  contract before slice 3; Codex wanted them last, behind the mutation log. Split
  out, by owner verdict. The advisor's contract items (priority, bias, staleness
  rules, "no answer" distinct from "empty", per-line lazy merge, an open step) and
  Codex's (validation, revisions, out-of-order completion) are inputs to the
  decorations spec.
- **Keys.** The red team and Codex accepted the rebuild; the advisor proposed key
  context gating. Gating is adopted: no rebuild, per-shell switches, multi-window
  ready.
- **Activation timing.** The red team: no work while hidden. The advisor and Codex:
  no main-thread time before the first frame, plus an after-first-frame trigger.
  Both are adopted (D6).

Wrong references in revision 1, corrected: the hide list is an array, not a
`match`; `ConfigChanged` is defined in `vault/events.rs:74` and subscribed in
`shell.rs:175`; the session needs no version bump.

### Round 2, on revision 2

An opus red team against the code, 2026-11-13. Codex was briefed too but hit its
usage limit before writing a report. Verdict: not buildable as written, two
blockers.

| Finding | Revision 3 |
|---|---|
| A command has no path to feature state (shell, context, `Window` and state borrowed at once) | state in its own entity, handlers on the handle, optional `Window` (D3) |
| `find_notes` by value cannot carry quick open; revision-1 Codex #13 renamed, not resolved | floor-2 `index()` and `vault()` accessors, owner verdict (B1, D4) |
| Backlinks writes back to the index (`forget`, `stale`, `cached`) | covered by `index()` (D4) |
| `follow(spot)` targets an unimplemented API | dropped until link landing lands (D4) |
| `Pane: Render + Focusable` is not dyn-compatible; backlinks is not focusable | `PaneEntry` erasure, optional focus (D5) |
| Files' special case in `change_layout` not generalized | `Pane::hidden`, focus landing (D5) |
| "Activate on the frame after the first" never fires under `TestWindow` or shots | `window.defer` from the first render (D6) |
| D9 step 4 targeted history, which never holds a view | parked slots and the closed stack, `⌘⇧T` test (D9) |
| Slice 1 underestimated; slice 0 is 31 files, not 23 | slice 1 split into 1a and 1b |
| Minors: ast-grep patterns, header buttons call shell methods, `PaneDecl` lacks the move action and selector, `tab_of` callers, no-vault mode, settings parse | D3, D5, D7, D9, D4 |

Verified correct by round 2: key context gating and the palette's key hints, the
adapters as the palette source, the settings order at launch and on `⌘O`, no
session format change, the modal close dropping its strong clone.

Wrong references in revision 2, corrected: `shell/tabs.rs:51,69,339` are `Place`,
`capture` and `display`, not history; slice 0's size; `follow(spot)`.

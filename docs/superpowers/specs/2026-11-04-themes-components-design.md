# Themes and components: semantic tokens, `ui` colours, vault themes

Status: **approved 2026-11-04** in a brainstorm; not built.

## Why

`theme.rs` holds 43 colour functions named by place (`tab_rule`,
`sidebar_text`), not by role. Five literals repeat (`#878580` three times,
`#cecdc3` three times, `#100f0f`, `#6a6a6a`, `#3c3c3c`). About 66 call sites
over 47 files read them as free functions, so nothing can change them at run
time. The docks spec comes next and would add more chrome written against this
shape. The user wants consistency across components and themes that users
write, one per vault.

## Decisions taken

- **D1. Full themes, dark and light.** Inkra ships Flexoki dark (today's look)
  and Flexoki light. A user theme file states its `appearance` and overrides
  any subset of tokens; the rest comes from the built-in theme of that
  appearance.
- **D2. Themes live in the vault.** Files go in `<vault>/.inkra/themes/*.json`;
  the choice goes in `<vault>/.inkra/settings.json`. A vault carries its own
  look and syncs it with its notes, as `.obsidian/themes` does.
- **D3. The choice is made in `⌘⇧P`.** `theme::Select` opens a picker:
  "System", the built-in themes, then the vault's. Moving the selection
  previews the theme live; Esc restores the previous one; Enter applies it and
  writes `settings.json`. With no choice, or `"theme": "system"`, the theme
  follows the system appearance.
- **D4. A gpui Global, with `&Theme` passed explicitly (Zed's model).**
  `GlobalTheme(Arc<Theme>)` is a `Global`; views read it with `cx.theme()`
  (trait `ActiveTheme` for `App`). Code paths with no `cx` take `&Theme` as an
  argument. A process-wide static was rejected: parallel `#[gpui::test]`s
  would share it.
- **D5. Flat semantic tokens, no primitive palette.** About 28 tokens, named by
  role (table below). A two-level palette (primitives, then tokens referring
  to them) can come later on top of this format without breaking it.
- **D6. A theme is colours only.** Sizes, radii and insets stay `const` in
  `theme/mod.rs`: the editor's and the tree's geometry depend on them
  (`row_center`, rows).
- **D7. Components take a semantic `Color`, never a raw colour.**
- **D8. A theme change redraws every window.** `apply` replaces the Global,
  bumps `revision` and calls `cx.refresh_windows()`, as Zed's `reload_theme`
  does. A change is rare and deliberate.
- **D9. Parsing is tolerant per key.** An unknown key or an invalid colour is
  skipped and reported; the base value stays. Only a JSON syntax error rejects
  the file. No error ever blocks the app.

## Tokens

Values are today's dark ones.

| Group | Token | Replaces |
|---|---|---|
| Surfaces | `background` | `sidebar_background`, `tab_bar_background` (`#181717f2`) |
| | `editor_background` | `editor_background`, `tab_active_background` (`#100f0f`) |
| | `elevated_background` | `modal_background` (`#252526`) |
| | `border` | `modal_border`, `group_rule` (`#3c3c3c`) |
| | `border_variant` | `tab_rule` (`#282726`) |
| Text | `text` | `sidebar_text`, `tab_text`, `icon_active` (`#cecdc3`) |
| | `text_selected` | `selected_row_text` (`#f0efe8`) |
| | `text_muted` | `icon_muted`, `tab_inactive_text`, `header_count_text`, `dirty_dot` |
| | `text_placeholder` | `placeholder_text` (`#6a6a6a`) |
| | `text_highlight` | `match_text` (`#e5c07b`) |
| Elements | `element_hover`, `element_selected` | `row_hover_background`, `selected_row_background` |
| | `chrome_element_hover` | `chrome_row_background` |
| | `ghost_element_hover` | `button_hover_background` |
| Accent | `accent` | `cursor`, `tab_drop_border` (`#4ea1ff`) |
| | `accent_background` | `tab_drop_background` |
| | `selection` | `selection` |
| Status | `error`, `error_background` | `failed_dot`, `rejected_text`, `notice_text`; `notice_background` |
| | `warning`, `warning_background` | `gone_banner_text`; `gone_banner_background` |
| Markdown | `md_text`, `md_heading`, `md_code`, `md_link`, `md_quote`, `md_quote_bar`, `md_marker` | `editor_text`, `heading_text`, `code_text`, `link_text`, `quote_text`, `quote_bar`, `marker_text` |

Three merges change pixels, accepted: `dirty_dot` `#8a8a8a` → `#878580`;
`rejected_text` `#e08a7a` and `notice_text` `#e8a0a0` → one `error`. `md_link`
stays apart from `accent`, so a theme can colour links unlike the caret.

Overlays stay theme values, not rules: `element_hover` is white at 5 % in dark
and black at about 4 % in light.

`shot_backdrop` is not a token: it stands in for the window blur in a shot and
follows `background` without its alpha.

## Model

```rust
pub(crate) struct Theme { name: SharedString, appearance: Appearance, colors: ThemeColors }
pub(crate) enum Appearance { Light, Dark }
struct GlobalTheme(Arc<Theme>);            // impl Global
pub(crate) trait ActiveTheme { fn theme(&self) -> &Theme; }   // impl for App
```

One `macro_rules! tokens { … }` lists the tokens once and generates
`ThemeColors`, `ThemeColors::set(&mut self, name: &str, Rgba) -> bool` and the
list of names. This replaces `#[derive(Refineable)]`: Zed's derive cannot skip
one invalid key without failing the file (Zed works around it with a parallel
`Option<String>` struct), and the macro needs no new dependency.

Files:

- `theme/mod.rs` — sizes, `Theme`, `GlobalTheme`, `ActiveTheme`
- `theme/colors.rs` — `tokens!`, `ThemeColors`
- `theme/builtin.rs` — Flexoki dark and light
- `theme/file.rs` — the JSON format and its tolerant parse
- `theme/markdown.rs` — `markdown_style(base, styles, &Theme)`
- `theme/store.rs` — `ThemeStore`
- `theme/picker.rs` — `ThemeDelegate`

## Components

- **`ui::Color`** — `Default, Selected, Muted, Placeholder, Highlight, Accent,
  Error, Warning`; `Color::rgba(self, &Theme) -> Rgba`, resolved at render. A
  subset of Zed's `ui::Color`; no `Custom` or `Player`, since nothing needs
  them.
- **`Icon::color(Color)`** replaces `impl Into<Hsla>`.
- **`Indicator`** resolves `Dirty → Muted`, `Failed → Error`.
- **`Label`** (new) — `Label::new(text).color(Color).size(LabelSize::Ui |
  Small)`. It is its own `UI_LINE_HEIGHT` line box, so a row holding it cannot
  clip it. It replaces the `text_color` + `text_size` + `line_height` triples
  in views: picker placeholders, the backlinks empty state, the header count.
- **`ListItem::style(ListItemStyle::Tree | Menu)`** replaces `inset()`.
  `Tree` sits on the chrome (`chrome_element_hover`, `TREE_ROW_HEIGHT`);
  `Menu` sits on an elevated surface (`element_hover`, `element_selected`).
- `IconButton` keeps `toggled` and `disabled`: they are states, not palettes.
- No generic `Button` or `Surface` yet: `IconButton` is the only button.

Components read `cx.theme()` in `render`. Views read `cx.theme().colors` only
for their own surfaces (a pane's background, a border); text and icons go
through `Color`.

`check.sh` fails on `rgb(0x` or `rgba(0x` outside `theme/` and `shot/`, so
CLAUDE.md's "never a literal" is checked, not only stated.

## Paths with no `cx`

These read colours today through free functions and take `&Theme` instead:

- `markdown_style`, called from `RunCache::run` (`editor/layout/runs.rs`) and
  `margin_width` (`editor/element.rs`).
- `Quads::new` (`text_input/paint.rs`), for the caret.
- the quote bar (`editor/layout/paint.rs`) and the selection quads
  (`editor/layout/mod.rs`).

`RunCache` caches `TextStyle`s, so it records the `revision` it built them at
and clears itself when the `revision` changes.

## File format

```json
{ "name": "Solarized", "appearance": "dark",
  "colors": { "accent": "#268bd2", "md_link": "#2aa198", "editor_background": "#002b36" } }
```

Colours are `#rgb`, `#rrggbb` or `#rrggbbaa`. `settings.json` is
`{ "theme": "Solarized" }`; a missing key or `"system"` follows the system.
Writing it reads the file as a `serde_json::Value`, sets `theme`, and writes it
back with `Fs::replace`, so keys this version does not know survive.

## Data flow

- **Startup.** `bootstrap` already runs `block_on(RealFs…)` before the first
  window (`bootstrap.rs:79-83`). It reads `settings.json` there and, if a theme
  is named, only that theme's file, then installs the resolved theme. The first
  frame is in the right theme: no flash. A missing or broken file falls back to
  the system theme, and the error reaches the notice once the window is open.
- **System appearance.** With no choice, `observe_window_appearance` switches
  between Flexoki dark and light.
- **`ThemeStore`** (entity) holds the theme list (built-ins, plus the vault's,
  listed when the picker opens), the chosen name and the `revision`.
  `apply(theme, cx)` does nothing when the colours are equal.
- **Hot reload.** `inkra_vault::Watch::changed` drops every dotted path today.
  It returns `Changed::Config(path)` for a path under `<root>/.inkra/` that is
  not a temporary file. The existing consumer (`app/src/vault/watch.rs`) routes
  it to `ThemeStore`: no second watcher. A change to the active theme's file or
  to `settings.json` reloads and applies. The echo of our own write reloads
  identical content, which `apply` ignores.
- **Picker.** `PickerDelegate` gains `selected_changed(ix, cx)`, empty by
  default. `ThemeDelegate` previews there (`apply` without writing), restores
  the previous theme on `Dismissed`, and applies and writes on confirm.

## Errors

| Case | Effect |
|---|---|
| JSON syntax error | file skipped; notice `theme "x.json": line 4: …` |
| unknown key, invalid colour | key skipped, base value kept; notice lists the keys |
| named theme not found | system theme; notice |
| `settings.json` write fails | theme applied for the session; notice, via the save path's retry |

## Deviations from Zed

- One `tokens!` macro instead of `Refineable` + an `Option<String>` content
  struct: per-key tolerance with one list of names (D9).
- About 28 tokens, not about 143: a note app, not an IDE.
- No `ThemeRegistry` Global, no theme families, no extensions: `ThemeStore`
  lists two built-ins and one folder.
- Themes are per vault, not per user (D2).
- The system appearance is observed, not read once (Zed's `SystemAppearance`
  is a snapshot).

## As built

The code (merged 2026-11-05, #94 and #88–#93, then #100) differs from the text above:

- No `ActiveTheme` trait: callers read `theme::current(cx)`.
- No `revision`: a change is `Arc::ptr_eq` on the Global's theme.
- No `RunCache`: layout reuse is `DocumentLayout::is_shaped_from`, which compares the theme.
- A theme's identity is its file stem; the file has no `name` key.
- `follows_system` lives in the Global.
- `install` replaces `apply`, and it pins the window appearance.
- `parse_color` is stricter than `Rgba::try_from`.
- Several notices join with "; ".
- A root `Rescan` also reloads the config.
- `ThemeStore` is a plain struct on `AppShell`, not an entity. It owns the choice: `pick` drops a reload in flight and chains the write after the previous one.
- `builtin::all()` is the one table of built-in names; `Choice` holds `"system"`.

## Assumption

The Global is one per process, and Inkra opens one vault per process. If two
windows ever show two vaults, the theme becomes per-window state. Not built
now.

## Testing

Success criterion for S1, written first: a script renders every shot scene
(`--list`) before and after the migration and diffs them. Only the three
merged tokens may differ (dirty dot, notice, rejected text).

- `theme` (unit):
  - Every token name round-trips through `set`.
  - A partial file inherits the base of its `appearance`.
  - An unknown key and an invalid colour are reported; other keys apply.
  - A JSON syntax error reports its line.
  - The colour parser round-trips under `proptest`.
- `inkra-vault`: `Watch::changed` returns `Config` under `.inkra/`. It still
  returns `None` for another dotted folder and for an `.inkra-` temporary file.
  Run `cargo mutants`.
- `app` (`#[gpui::test]`, `MemFs`):
  - `apply` changes the background the editor reads, and `RunCache` rebuilds.
    The test fails if the `revision` check is removed.
  - Writing `settings.json` keeps an unknown key.
  - An event on the active theme reloads it, and an identical echo does not
    refresh.
- Keys (`simulate_keystrokes`): `⌘⇧P`, "theme", `↓` (preview applied), Esc
  (previous theme back); then again with Enter (`settings.json` written).
- Shots: every scene in dark and light, looked at before a slice is done.
- To check in S3: whether gpui's `TestPlatform` can simulate an appearance
  change. If not, test the handler by a direct call.

## Slices

| # | Slice | Content |
|---|---|---|
| S1 | Tokens and Global | `tokens!`, `Theme`, `GlobalTheme`, `cx.theme()`, Flexoki dark, the ~66 call sites, `&Theme` in `markdown_style`, `RunCache` and `Quads`, `revision`, the `check.sh` guard, the `theme/` split |
| S2 | Components | `Color`, `Label`, `ListItemStyle`, `Icon` and `Indicator` on `Color`, views moved to `Label` |
| S3 | Light and system | Flexoki light, `observe_window_appearance`, shots in both themes, the nav row's dimmed `→` contrast |
| S4 | User files | JSON format, tolerant parse, `settings.json`, the startup load, errors in the notice |
| S5 | Hot reload | `Changed::Config` in `inkra-vault`, routing to `ThemeStore`, the echo ignored |
| S6 | Picker | `theme::Select`, `ThemeDelegate`, `selected_changed`, restore on Esc, writing the choice |

S1 is the one large slice: mechanical, and checked by the shot diff. S2–S6
each aim at about 150 lines.

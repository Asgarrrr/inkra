# gpui-test

Prototype : éditeur de texte full-Rust en GPUI, pour évaluer la viabilité
d'un remplacement de Tauri sur Inkra. Design complet :
[docs/superpowers/specs/2026-09-19-gpui-test-architecture-design.md](./docs/superpowers/specs/2026-09-19-gpui-test-architecture-design.md).

## Commands

- build: `cargo build --workspace`
- test: `cargo test --workspace`
- lint: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- fmt: `cargo fmt --all -- --check`
- run: `cargo run -p gpui-test-app`

## GPUI Documentation (Hard Rule)

Before writing or reviewing any code that touches `gpui`, check the local
vendored source first — not training memory, not web search:

- Source: `~/.cargo/registry/src/index.crates.io-*/gpui-0.2.2/src/` (exact
  version pinned in `Cargo.lock`) — the most reliable reference, read/grep
  it directly.
- Rendered docs: `cargo doc -p gpui --no-deps`, then open
  `target/doc/gpui/index.html`.

Zed's own usage of `gpui` — especially `crates/editor/src/input.rs`
(`EntityInputHandler`, IME marked ranges, UTF-16 offset conversion) — is
mirrored locally at `vendor/zed-reference/crates/{gpui,editor,text,language}`
(sparse shallow clone, gitignored, not part of this repo's history). Consult
it before implementing any input/cursor/selection code — it's a working,
production-hardened solution to exactly this problem. Refresh with
`cd vendor/zed-reference && git pull` if it goes stale.

Fall back to DeepWiki/web search only for what neither local source can
answer: version history, GitHub issues/discussions, or design rationale
not visible in the code itself.

## Architecture (Hard Rule)

Workspace Cargo, deux crates, dépendance à sens unique :

`crates/core` → `crates/app`

- `core` : buffer de texte, insert/delete, offsets (newtypes byte/char),
  undo. **Zéro dépendance GPUI.** Ne contient PAS le mouvement de
  curseur ni la sélection — ça dépend du layout (wrapping, glyphes), qui
  n'existe que côté GPUI.
- `app` : binaire. Wrapper `Entity<T>` GPUI autour du buffer `core`,
  mouvement/sélection, implémentation `EntityInputHandler` (conversion
  offsets UTF-16 ↔ offsets `core`), `main()`.

Ne jamais ajouter une dépendance qui inverse ce graphe. Vérifié par
`scripts/check.sh` (isolation check `cargo tree`, couvre normal/build/dev
et toutes les cibles — durci après plusieurs contournements trouvés en
red team, ne pas revenir à une commande `cargo tree` plus simple).

En cas de doute sur la frontière d'un module : le mettre dans `core`
seulement s'il ne dépend d'aucune info de layout ; sinon `app`.

## Conventions

- Newtypes pour les offsets (byte, char côté `core` ; UTF-16 côté `app`
  pour le protocole GPUI). Ne jamais les mélanger.
- Pas de `thiserror`/`Result` sur les opérations de buffer infaillibles
  (offset invalide = bug appelant → `debug_assert!`/panic). `thiserror`
  réservé aux vraies frontières faillibles (I/O fichier, à venir).
- `[workspace.lints]` : `unsafe_code = deny`, `unreachable_pub = warn`,
  `clippy::all`/`pedantic = warn`, `clippy::unwrap_used`/`expect_used =
  deny` (override `#[cfg_attr(test, allow(clippy::unwrap_used))]` dans
  les modules de test).

## Testing

- `core` : `#[test]` + `proptest` sur les séquences d'édition (modèle de
  référence `Vec<char>`). Commit `.proptest-regressions` dès le premier
  test.
- `app` : `gpui::TestAppContext` / `#[gpui::test]` (headless, pas de
  fenêtre réelle). Pas de snapshot/visual testing tant que l'UI reste un
  prototype instable.

## Don't recreate — it already exists

| Need | Use | Where |
|---|---|---|
| _(vide pour l'instant — projet neuf)_ | | |

Avant de créer un fichier, helper, ou type : vérifier cette table. Quand
vous construisez quelque chose de réutilisable, ajoutez-le ici dans le
même changement.

## Verification

Définition de "done" : `./scripts/check.sh` tout vert (fmt, clippy -D
warnings, isolation gpui, test, build --release).

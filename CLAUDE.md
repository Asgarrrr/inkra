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

Ne jamais ajouter une dépendance qui inverse ce graphe. Vérifié en CI par
`cargo tree -p gpui-test-core -i gpui` (doit échouer/retourner vide).

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

Définition de "done" : `cargo fmt --all -- --check`, `cargo clippy
--workspace --all-targets --all-features -- -D warnings`, `cargo tree -p
gpui-test-core -i gpui` (vide), `cargo test --workspace`, `cargo build
--workspace --release` — tous verts.

//! `LiveIndex`'s builds at 50,000 entries: the first, a layout, and a rebuild that finds only
//! stamps. `assets` holds one note in ten, the rest images (file kinds spec, Performance gate).

#![expect(
    clippy::expect_used,
    reason = "a harness setup failure should panic; allow-expect-in-tests skips helpers outside #[test] and #[cfg(test)]"
)]

use std::path::Path;

use criterion::{Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_index::LiveIndex;
use inkra_index::live::{Effects, Turn};
use inkra_vault::Vault;
use inkra_vault::mem_fs::MemFs;

const FILES: usize = 50_000;
const FOLDERS: usize = 100;

fn vault(layout: &str) -> Vault {
    let fs = MemFs::new();
    for file in 0..FILES {
        let name = if layout == "assets" && file % 10 != 0 {
            format!("a{file}.png")
        } else {
            format!("n{file}.md")
        };
        fs.insert_file(format!("/vault/f{}/{name}", file % FOLDERS), "");
    }
    block_on(Vault::scan(&fs, Path::new("/vault"))).expect("a MemFs scan")
}

/// A vault change on an idle index, its build, and the build's apply; the loads are dropped.
fn rebuild(live: &mut LiveIndex, vault: &Vault) -> Effects {
    let ((), effects) = live.turn(vault, |turn: &mut Turn<'_>| turn.vault_changed());
    let built = effects.build.expect("an idle index builds").run();
    live.turn(vault, |turn| turn.built(built)).1
}

fn builds(c: &mut Criterion) {
    for layout in ["nested", "assets"] {
        let vault = vault(layout);
        c.bench_function(&format!("live layout {layout} 50k"), |b| {
            b.iter_with_large_drop(|| {
                let mut live = LiveIndex::new(4);
                let ((), effects) = live.turn(&vault, |turn: &mut Turn<'_>| turn.vault_changed());
                let built = effects.build.expect("a scanned vault builds").run();
                (live, built)
            });
        });
        // The first build is a layout; every later one finds the same notes, so `Stamps`.
        let mut live = LiveIndex::new(4);
        drop(rebuild(&mut live, &vault));
        c.bench_function(&format!("live stamps {layout} 50k"), |b| {
            b.iter_with_large_drop(|| rebuild(&mut live, &vault));
        });
    }
}

criterion_group!(benches, builds);
criterion_main!(benches);

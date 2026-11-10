//! `walk`, `applied` and a re-key on a shared state at 50,000 files, on `RealFs` (open-first spec, Performance).
//! `assets` holds one note in ten, the rest images (file kinds spec, Performance gate).

#![expect(
    clippy::expect_used,
    reason = "a harness setup failure should panic; allow-expect-in-tests skips helpers outside #[test] and #[cfg(test)]"
)]

use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::Duration;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_vault::{Fs, Job, Jobs, Kind, Mtime, Reach, RealFs, RelPath, Serial, Stamp, Vault, Walk};

const NOTES: usize = 50_000;
const FOLDERS: usize = 100;
const WRITTEN: &str = ".written";

fn tree(layout: &str, folders: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("inkra-vault-bench-{layout}"));
    if dir.join(WRITTEN).exists() {
        return dir;
    }
    let _ = std::fs::remove_dir_all(&dir);
    for note in 0..NOTES {
        let folder = if folders == 0 {
            dir.clone()
        } else {
            dir.join(format!("f{}", note % folders))
        };
        std::fs::create_dir_all(&folder).expect("folder");
        let name = if layout == "assets" && note % 10 != 0 {
            format!("a{note}.png")
        } else {
            format!("n{note}.md")
        };
        std::fs::write(folder.join(name), "").expect("file");
    }
    std::fs::write(dir.join(WRITTEN), "").expect("marker");
    dir
}

/// Runs each job on its own thread, as the app's executor does.
struct Threads(usize);

impl Jobs for Threads {
    fn parallelism(&self) -> usize {
        self.0
    }

    fn run<'a>(&'a self, jobs: Vec<Job<'a>>) -> Job<'a> {
        Box::pin(async move {
            std::thread::scope(|scope| {
                for job in jobs {
                    scope.spawn(move || block_on(job));
                }
            });
        })
    }
}

fn walks(c: &mut Criterion) {
    let threads = Threads(
        std::thread::available_parallelism()
            .expect("a core count")
            .get(),
    );
    for (layout, folders) in [("nested", FOLDERS), ("flat", 0)] {
        let root = tree(layout, folders);
        c.bench_function(&format!("walk serial {layout} 50k"), |b| {
            b.iter(|| {
                block_on(Vault::walk(&RealFs, &root, &RelPath::root(), &Serial)).expect("walk")
            });
        });
        c.bench_function(&format!("walk parallel {layout} 50k"), |b| {
            b.iter(|| {
                block_on(Vault::walk(&RealFs, &root, &RelPath::root(), &threads)).expect("walk")
            });
        });
    }
    let root = tree("assets", FOLDERS);
    c.bench_function("walk serial assets 50k", |b| {
        b.iter(|| block_on(Vault::walk(&RealFs, &root, &RelPath::root(), &Serial)).expect("walk"));
    });
    // The initial scan's full pass after its root pass: the root is not listed again.
    let root = tree("flat", 0);
    let listing = block_on(RealFs.read_dir(&root)).expect("the root");
    c.bench_function("walk seeded flat 50k", |b| {
        b.iter_batched(
            || Walk {
                read: vec![RelPath::root()],
                found: listing.clone(),
                reach: Reach::Partial,
            },
            |seed| block_on(Vault::walk_seeded(&RealFs, &root, seed, &threads)),
            BatchSize::LargeInput,
        );
    });
}

fn apply(c: &mut Criterion) {
    for layout in ["nested", "assets"] {
        let root = tree(layout, FOLDERS);
        let walk =
            || block_on(Vault::walk(&RealFs, &root, &RelPath::root(), &Serial)).expect("walk");
        let empty = Vault::new(&root);
        c.bench_function(&format!("applied from empty {layout} 50k"), |b| {
            b.iter_batched(
                walk,
                |walk| black_box(empty.applied(walk)),
                criterion::BatchSize::LargeInput,
            );
        });
        // A miss reads every entry: a create's or a rename's check.
        let scanned = empty.applied(walk());
        let folder = scanned
            .entries()
            .find(|e| e.path.as_path() == Path::new("f0"))
            .expect("f0")
            .id;
        c.bench_function(&format!("name_taken {layout} 50k"), |b| {
            b.iter(|| black_box(scanned.name_taken(Some(folder), Kind::Note, "missing", None)));
        });
    }
    let root = tree("nested", FOLDERS);
    let walk = || block_on(Vault::walk(&RealFs, &root, &RelPath::root(), &Serial)).expect("walk");
    let scanned = Vault::new(&root).applied(walk());
    c.bench_function("applied again nested 50k", |b| {
        b.iter_batched(
            walk,
            |walk| black_box(scanned.applied(walk)),
            criterion::BatchSize::LargeInput,
        );
    });
    // The scan task's test that an echo's walk changed nothing: two equal states, no shared `Arc`.
    let again = scanned.applied(walk());
    c.bench_function("unchanged eq nested 50k", |b| {
        b.iter(|| black_box(again == scanned));
    });
    // A save outside a scan: the re-key's `retain` walks the whole identity map on the foreground.
    let note = scanned
        .entries()
        .find(|e| e.path.as_path().extension().is_some())
        .expect("a note");
    let loaded = block_on(RealFs.load(&root.join(note.path.as_path()))).expect("load");
    let (id, identity, stamp) = (note.id, loaded.identity, loaded.stamp);
    let mut live = scanned.applied(walk()); // its own state, shared with nothing
    c.bench_function("reidentify 50k", |b| {
        b.iter(|| live.reidentify(id, identity, stamp));
    });
    // A save while an `IndexSync` build holds the entries: a new mtime copies them.
    let later = Stamp {
        mtime: Mtime(stamp.mtime.0 + Duration::from_secs(1)),
        ..stamp
    };
    let mut flip = false;
    c.bench_function("reidentify shared 50k", |b| {
        b.iter_with_large_drop(|| {
            let held = live.shared_entries();
            flip = !flip;
            let moved = live.reidentify(id, identity, if flip { later } else { stamp });
            (held, moved)
        });
    });
}

criterion_group!(benches, walks, apply);
criterion_main!(benches);

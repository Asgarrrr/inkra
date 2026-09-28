//! `walk`, `applied` and a re-key on a shared state at 50,000 notes, on `RealFs` (open-first spec, Performance).

#![allow(clippy::expect_used)]

use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use futures::executor::block_on;
use inkra_vault::{Fs, Job, Jobs, Mtime, Reach, RealFs, RelPath, Serial, Vault, Walk};

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
        std::fs::write(folder.join(format!("n{note}.md")), "").expect("note");
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
    let root = tree("nested", FOLDERS);
    let walk = || block_on(Vault::walk(&RealFs, &root, &RelPath::root(), &Serial)).expect("walk");
    let empty = Vault::new(&root);
    c.bench_function("applied from empty nested 50k", |b| {
        b.iter_batched(
            walk,
            |walk| black_box(empty.applied(walk)),
            criterion::BatchSize::LargeInput,
        );
    });
    let scanned = empty.applied(walk());
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
    let (id, identity, mtime) = (note.id, loaded.identity, loaded.stamp.mtime);
    let mut live = scanned.applied(walk()); // its own state, shared with nothing
    c.bench_function("reidentify 50k", |b| {
        b.iter(|| live.reidentify(id, identity, mtime));
    });
    // A save while an `IndexSync` build holds the entries: a new mtime copies them.
    let later = Mtime(mtime.0 + Duration::from_secs(1));
    let mut flip = false;
    c.bench_function("reidentify shared 50k", |b| {
        b.iter_with_large_drop(|| {
            let held = live.shared_entries();
            flip = !flip;
            let moved = live.reidentify(id, identity, if flip { later } else { mtime });
            (held, moved)
        });
    });
}

criterion_group!(benches, walks, apply);
criterion_main!(benches);

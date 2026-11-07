//! The parallel walk: jobs on threads, a folder still being read, a job that panics.

use std::panic::{self, AssertUnwindSafe};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use futures::executor::block_on;

use super::test_helpers::paths;
use super::*;
use crate::fs::{DirEntry, Loaded, Saved, Stamp};
use crate::mem_fs::MemFs;
use crate::watch::Watch;
use crate::{Job, Jobs};

const ROOT: &str = "/vault";

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

#[test]
fn a_parallel_walk_reads_what_a_serial_one_reads() {
    let fs = MemFs::new();
    for n in 0..200 {
        fs.insert_file(format!("/vault/f{}/g{}/n{n}.md", n % 7, n % 3), "");
    }
    let everything = RelPath::root();
    let serial = block_on(Vault::walk(&fs, Path::new(ROOT), &everything, &Serial)).expect("walk");
    let parallel =
        block_on(Vault::walk(&fs, Path::new(ROOT), &everything, &Threads(8))).expect("walk");

    let base = Vault::new(Path::new(ROOT));
    let (serial, parallel) = (base.applied(serial), base.applied(parallel));
    assert_eq!(paths(&serial), paths(&parallel));
    let ids = |vault: &Vault| vault.entries().map(|e| e.id).collect::<Vec<_>>();
    assert_eq!(
        ids(&serial),
        ids(&parallel),
        "ids follow path order, not read order"
    );
}

#[test]
fn a_parallel_walk_waits_for_a_folder_still_being_read() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/n.md", "");
    fs.insert_file("/vault/f/h/m.md", "");
    let f = fs.block_read_dir("/vault/f");
    let (g, h) = (
        fs.block_read_dir("/vault/f/g"),
        fs.block_read_dir("/vault/f/h"),
    );
    // Released from another thread: the jobs block this one until the walk ends.
    let (walk, both_started) = std::thread::scope(|scope| {
        let fs = &fs;
        let helper = scope.spawn(move || {
            f.release();
            let started = || {
                fs.read_dirs(Path::new("/vault/f/g")) > 0
                    && fs.read_dirs(Path::new("/vault/f/h")) > 0
            };
            // Bounded, so a job that quit fails the assertion instead of hanging the test.
            let deadline = Instant::now() + Duration::from_secs(5);
            while !started() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            let both = started();
            g.release();
            h.release();
            both
        });
        let walk = block_on(Vault::walk(
            fs,
            Path::new(ROOT),
            &RelPath::root(),
            &Threads(4),
        ));
        (walk, helper.join().expect("the helper"))
    });

    assert!(both_started, "a job waited for f instead of quitting");
    assert_eq!(walk.expect("walk").found.len(), 5, "f, g, h, n.md and m.md");
}

/// A `MemFs` whose listing of one folder panics, as a bug in an `Fs` would.
struct PanickingFs {
    inner: MemFs,
    panics_on: &'static str,
}

#[async_trait::async_trait]
impl Fs for PanickingFs {
    async fn read_dir(&self, path: &Path) -> Result<Vec<DirEntry>> {
        assert_ne!(path, Path::new(self.panics_on), "the listing panics");
        self.inner.read_dir(path).await
    }

    async fn list_files(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        self.inner.list_files(dir).await
    }

    async fn check_dir(&self, path: &Path) -> Result<()> {
        self.inner.check_dir(path).await
    }

    async fn load(&self, path: &Path) -> Result<Loaded> {
        self.inner.load(path).await
    }

    async fn read_bytes(&self, path: &Path, max_len: u64) -> Result<Vec<u8>> {
        self.inner.read_bytes(path, max_len).await
    }

    async fn save(&self, path: &Path, text: &str, expected: Stamp) -> Result<Saved> {
        self.inner.save(path, text, expected).await
    }

    async fn create_parents(&self, path: &Path) -> Result<()> {
        self.inner.create_parents(path).await
    }

    async fn replace(&self, path: &Path, text: &str) -> Result<()> {
        self.inner.replace(path, text).await
    }

    async fn replace_bytes(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        self.inner.replace_bytes(path, bytes).await
    }

    async fn remove_file(&self, path: &Path) -> Result<()> {
        self.inner.remove_file(path).await
    }

    async fn create(&self, path: &Path, kind: Kind) -> Result<FileIdentity> {
        self.inner.create(path, kind).await
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.inner.rename(from, to).await
    }

    async fn trash(&self, path: &Path) -> Result<()> {
        self.inner.trash(path).await
    }

    fn watch(&self, root: &Path) -> Result<Watch> {
        self.inner.watch(root)
    }
}

#[test]
fn a_job_that_panics_ends_the_walk_instead_of_hanging_it() {
    let fs = PanickingFs {
        inner: MemFs::new(),
        panics_on: "/vault/boom",
    };
    fs.inner.insert_file("/vault/boom/a.md", "");
    fs.inner.insert_file("/vault/fine/b.md", "");
    let (ended, outcome) = mpsc::channel();
    // Detached, so a walk that hangs fails the test at the timeout instead of hanging it.
    std::thread::spawn(move || {
        let walked = panic::catch_unwind(AssertUnwindSafe(|| {
            block_on(Vault::walk(
                &fs,
                Path::new(ROOT),
                &RelPath::root(),
                &Threads(2),
            ))
        }));
        let _ = ended.send(walked.is_err());
    });

    let panicked = outcome
        .recv_timeout(Duration::from_secs(5))
        .expect("the walk ended");
    assert!(panicked, "the job's panic reaches the walk's caller");
}

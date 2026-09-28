use std::path::Path;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use futures::executor::block_on;

use super::MemFs;
use crate::fs::{Fs, Kind};
use crate::watch::{PathEvent, Watch};

fn watch(fs: &MemFs) -> Watch {
    fs.insert_dir("/vault");
    fs.watch(Path::new("/vault")).expect("watch")
}

/// The events queued so far, as `(path, rescan)`.
fn drain(events: &mut UnboundedReceiver<PathEvent>) -> Vec<(String, bool)> {
    std::iter::from_fn(|| events.try_recv().ok())
        .map(|event| (event.path.to_string_lossy().into_owned(), event.rescan))
        .collect()
}

fn changed(paths: &[&str]) -> Vec<(String, bool)> {
    paths
        .iter()
        .map(|path| ((*path).to_owned(), false))
        .collect()
}

#[test]
fn a_watch_names_its_root_twice_when_nothing_resolves_it() {
    let fs = MemFs::new();

    let watch = watch(&fs);

    assert_eq!(watch.root, Path::new("/vault"));
    assert_eq!(watch.resolved, Path::new("/vault"));
}

#[test]
fn insert_file_sends_the_note_and_every_folder_it_created() {
    let fs = MemFs::new();
    let mut watch = watch(&fs);

    fs.insert_file("/vault/f/g/a.md", "a");
    fs.insert_file("/vault/f/g/a.md", "again");

    assert_eq!(
        drain(&mut watch.events),
        changed(&[
            "/vault/f/g",
            "/vault/f",
            "/vault/f/g/a.md",
            "/vault/f/g/a.md"
        ])
    );
}

#[test]
fn every_injector_sends_the_path_it_touched() {
    let fs = MemFs::new();
    let mut watch = watch(&fs);
    let identity = crate::fs::FileIdentity {
        device: 1,
        inode: 999,
    };

    fs.insert_dir("/vault/d");
    fs.insert_file_with_identity("/vault/b.md", "b", identity);
    fs.hard_link(Path::new("/vault/b.md"), "/vault/c.md");
    fs.remove(Path::new("/vault/b.md"));

    assert_eq!(
        drain(&mut watch.events),
        changed(&["/vault/d", "/vault/b.md", "/vault/c.md", "/vault/b.md"])
    );
}

#[test]
fn create_rename_and_trash_send_every_path_they_touched() {
    let fs = MemFs::new();
    let mut watch = watch(&fs);

    block_on(fs.create(Path::new("/vault/a.md"), Kind::Note)).expect("create");
    block_on(fs.rename(Path::new("/vault/a.md"), Path::new("/vault/b.md"))).expect("rename");
    block_on(fs.trash(Path::new("/vault/b.md"))).expect("trash");

    assert_eq!(
        drain(&mut watch.events),
        changed(&["/vault/a.md", "/vault/a.md", "/vault/b.md", "/vault/b.md"])
    );
}

#[test]
fn a_failed_mutation_sends_nothing() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let mut watch = watch(&fs);
    fs.make_unwritable("/vault/a.md");
    let stamp = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;

    assert!(block_on(fs.create(Path::new("/vault/a.md"), Kind::Note)).is_err());
    assert!(block_on(fs.rename(Path::new("/vault/x.md"), Path::new("/vault/y.md"))).is_err());
    assert!(block_on(fs.trash(Path::new("/vault/x.md"))).is_err());
    assert!(block_on(fs.save(Path::new("/vault/a.md"), "b", stamp)).is_err());

    assert_eq!(drain(&mut watch.events), []);
}

/// As `RealFs`: the temporary file, then the note it is renamed onto.
#[test]
fn a_save_sends_its_temporary_file_then_the_note() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let mut watch = watch(&fs);
    let stamp = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;

    block_on(fs.save(Path::new("/vault/a.md"), "b", stamp)).expect("save");

    let events = drain(&mut watch.events);
    assert_eq!(events.len(), 2, "{events:?}");
    let (temporary, rescan) = &events[0];
    assert!(!rescan);
    assert!(
        temporary.starts_with("/vault/a.md.inkra-")
            && Path::new(temporary).extension() == Some("tmp".as_ref()),
        "{temporary}"
    );
    assert_eq!(events[1], ("/vault/a.md".to_owned(), false));
}

#[test]
fn a_save_that_preserves_sends_the_preserved_file_too() {
    let fs = MemFs::new();
    fs.insert_file("/vault/a.md", "a");
    let stale = block_on(fs.load(Path::new("/vault/a.md")))
        .expect("load")
        .stamp;
    fs.insert_file("/vault/a.md", "theirs");
    let mut watch = watch(&fs);

    let saved = block_on(fs.save(Path::new("/vault/a.md"), "ours", stale)).expect("save");

    let preserved = saved.preserved.expect("the bytes it replaced");
    let events = drain(&mut watch.events);
    assert_eq!(events.len(), 3, "{events:?}");
    assert_eq!(events[0], (preserved.to_string_lossy().into_owned(), false));
    assert_eq!(events[2], ("/vault/a.md".to_owned(), false));
}

#[test]
fn send_rescan_sends_a_rescan_event() {
    let fs = MemFs::new();
    let mut watch = watch(&fs);

    fs.send_rescan("/vault/f");

    assert_eq!(drain(&mut watch.events), [("/vault/f".to_owned(), true)]);
}

#[test]
fn a_path_outside_the_root_sends_nothing() {
    let fs = MemFs::new();
    let mut watch = watch(&fs);

    fs.insert_file("/elsewhere/a.md", "a");
    fs.insert_file("/vaulted.md", "a");

    assert_eq!(drain(&mut watch.events), []);
}

#[test]
fn every_watch_gets_every_event() {
    let fs = MemFs::new();
    let mut first = watch(&fs);
    let mut second = fs.watch(Path::new("/vault")).expect("watch");

    fs.insert_file("/vault/a.md", "a");

    assert_eq!(drain(&mut first.events), changed(&["/vault/a.md"]));
    assert_eq!(drain(&mut second.events), changed(&["/vault/a.md"]));
}

#[test]
fn dropping_the_watcher_ends_the_stream() {
    let fs = MemFs::new();
    let Watch {
        mut events,
        watcher,
        ..
    } = watch(&fs);

    fs.insert_file("/vault/a.md", "a");
    drop(watcher);
    fs.insert_file("/vault/b.md", "b");

    let rest: Vec<_> = block_on(events.by_ref().collect::<Vec<_>>())
        .into_iter()
        .map(|event| event.path)
        .collect();
    assert_eq!(rest, [Path::new("/vault/a.md")]);
}

/// As `rm -r`: the folder goes with everything in it, one event per path.
#[test]
fn removing_a_folder_removes_its_subtree_and_sends_every_path() {
    let fs = MemFs::new();
    fs.insert_file("/vault/f/g/a.md", "a");
    fs.insert_file("/vault/f/b.md", "b");
    fs.insert_file("/vault/fx.md", "x");
    let mut watch = watch(&fs);

    fs.remove(Path::new("/vault/f"));
    fs.remove(Path::new("/vault/none"));

    assert_eq!(
        drain(&mut watch.events),
        changed(&["/vault/f", "/vault/f/b.md", "/vault/f/g", "/vault/f/g/a.md"])
    );
    fs.insert_dir("/vault/f/g");
    let listed = block_on(fs.read_dir(Path::new("/vault/f/g"))).expect("read_dir");
    assert!(listed.is_empty(), "nothing it held comes back");
    assert!(fs.read_to_string(Path::new("/vault/fx.md")).is_some());
}

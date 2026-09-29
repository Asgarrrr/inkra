use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use inkra_vault::Mtime;
use proptest::prelude::*;

use super::{Lifecycle, Location, PendingSave, Swap};

fn path() -> PathBuf {
    PathBuf::from("/vault/a.md")
}

fn mtime(secs: u64) -> Mtime {
    Mtime(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}

fn open(location: Location) -> Lifecycle<()> {
    Lifecycle::Open(location, ())
}

fn trashing(location: Location) -> Lifecycle<()> {
    Lifecycle::Trashing(Some((location, ())))
}

fn gone() -> Location {
    Location::Gone(path())
}

fn detached() -> Location {
    Location::Detached(path())
}

#[test]
fn a_present_note_deleted_on_disk_goes_gone_at_its_last_path() {
    let mut life = open(Location::Vault);
    let swap = life.swapped(None, Some(mtime(1)), || Some(path()));
    assert_eq!(
        swap,
        Swap {
            flipped: true,
            reload: false
        }
    );
    assert_eq!(life, open(gone()));
    assert!(life.is_gone());
}

#[test]
fn no_last_path_leaves_the_note_present() {
    let mut life = open(Location::Vault);
    assert_eq!(life.swapped(None, Some(mtime(1)), || None), Swap::NONE);
    assert_eq!(life, open(Location::Vault));
}

#[test]
fn a_gone_note_whose_entry_returns_comes_back_and_reloads() {
    let mut life = open(gone());
    let swap = life.swapped(Some(mtime(1)), None, || unreachable!());
    assert_eq!(
        swap,
        Swap {
            flipped: true,
            reload: true
        }
    );
    assert_eq!(life, open(Location::Vault));
    assert!(!life.is_gone());
}

#[test]
fn a_gone_note_with_no_entry_stays_gone_without_asking_its_path() {
    let mut life = open(gone());
    assert_eq!(life.swapped(None, None, || unreachable!()), Swap::NONE);
    assert_eq!(life, open(gone()));
}

#[test]
fn our_own_trash_is_never_gone() {
    let mut life = trashing(Location::Vault);
    assert_eq!(
        life.swapped(None, Some(mtime(1)), || Some(path())),
        Swap::NONE
    );
    assert_eq!(life, trashing(Location::Vault));
}

#[test]
fn a_trashing_gone_note_whose_entry_returns_flips_back() {
    let mut life = trashing(gone());
    let swap = life.swapped(Some(mtime(1)), None, || unreachable!());
    assert_eq!(
        swap,
        Swap {
            flipped: true,
            reload: true
        }
    );
    assert_eq!(life, trashing(Location::Vault));
}

#[test]
fn a_detached_note_is_outside_the_swap() {
    for (now, before) in [(None, None), (Some(mtime(1)), None), (None, Some(mtime(1)))] {
        let mut life = open(detached());
        assert_eq!(life.swapped(now, before, || Some(path())), Swap::NONE);
        assert_eq!(life, open(detached()));
    }
}

#[test]
fn a_note_that_is_not_bound_is_outside_the_swap() {
    for mut life in [Lifecycle::<()>::Closed, Lifecycle::Trashing(None)] {
        let before = life.clone();
        assert_eq!(
            life.swapped(None, Some(mtime(1)), || Some(path())),
            Swap::NONE
        );
        assert_eq!(
            life.swapped(Some(mtime(2)), None, || Some(path())),
            Swap::NONE
        );
        assert_eq!(life, before);
    }
}

#[test]
fn only_a_changed_mtime_reloads() {
    let mut life = open(Location::Vault);
    assert_eq!(
        life.swapped(Some(mtime(1)), Some(mtime(1)), || None),
        Swap::NONE
    );
    let changed = Swap {
        flipped: false,
        reload: true,
    };
    assert_eq!(
        life.swapped(Some(mtime(2)), Some(mtime(1)), || None),
        changed
    );
    assert_eq!(life.swapped(Some(mtime(2)), None, || None), changed);
    assert_eq!(life, open(Location::Vault));
}

#[test]
fn a_trashing_note_whose_file_changed_still_asks_a_reload() {
    let mut life = trashing(Location::Vault);
    let swap = life.swapped(Some(mtime(2)), Some(mtime(1)), || None);
    assert_eq!(
        swap,
        Swap {
            flipped: false,
            reload: true
        }
    );
    assert!(!life.is_open(), "the reload's first step stops on it");
}

#[test]
fn a_present_note_with_an_entry_is_preempted() {
    let mut life = open(Location::Vault);
    assert_eq!(life.begin_trash(true), Some(PendingSave::Write));
    assert_eq!(life, trashing(Location::Vault));
}

#[test]
fn a_present_note_with_no_entry_is_refused() {
    let mut life = open(Location::Vault);
    assert_eq!(life.begin_trash(false), None);
    assert_eq!(life, open(Location::Vault));
}

#[test]
fn a_gone_note_drops_its_pending_save() {
    for has_entry in [false, true] {
        let mut life = open(gone());
        assert_eq!(life.begin_trash(has_entry), Some(PendingSave::Drop));
        assert_eq!(life, trashing(gone()));
    }
}

#[test]
fn a_closed_note_with_an_entry_is_trashed() {
    let mut life = Lifecycle::<()>::Closed;
    assert_eq!(life.begin_trash(true), Some(PendingSave::Write));
    assert_eq!(life, Lifecycle::Trashing(None));
}

#[test]
fn a_closed_note_with_no_entry_is_refused() {
    let mut life = Lifecycle::<()>::Closed;
    assert_eq!(life.begin_trash(false), None);
    assert_eq!(life, Lifecycle::Closed);
}

#[test]
fn a_second_trash_is_refused() {
    for before in [
        trashing(Location::Vault),
        trashing(gone()),
        Lifecycle::Trashing(None),
    ] {
        let mut life = before.clone();
        assert_eq!(life.begin_trash(true), None);
        assert_eq!(life, before);
    }
}

#[test]
fn a_detached_note_is_never_trashed() {
    for has_entry in [false, true] {
        let mut life = open(detached());
        assert_eq!(life.begin_trash(has_entry), None);
        assert_eq!(life, open(detached()));
    }
}

#[test]
fn a_failed_trash_restores_the_note() {
    for (before, has_entry) in [
        (open(Location::Vault), true),
        (open(gone()), false),
        (open(gone()), true),
        (Lifecycle::Closed, true),
    ] {
        let mut life = before.clone();
        assert!(life.begin_trash(has_entry).is_some());
        life.trash_failed();
        assert_eq!(life, before);
    }
}

#[test]
fn a_failed_trash_leaves_every_other_state() {
    for before in [Lifecycle::Closed, open(Location::Vault), open(gone())] {
        let mut life = before.clone();
        life.trash_failed();
        assert_eq!(life, before);
    }
}

#[test]
fn a_done_trash_is_sticky_and_closes() {
    let mut life = Lifecycle::Trashing(Some((Location::Vault, 7)));
    assert_eq!(life.trash_done(), Some(7));
    assert_eq!(life, Lifecycle::Trashing(None));
    assert_eq!(life.trash_done(), None);
    assert_eq!(life.begin_trash(true), None);
    assert!(life.bound().is_none());
    assert!(!life.is_open());
}

#[test]
fn a_done_trash_leaves_every_other_state() {
    for before in [Lifecycle::Closed, Lifecycle::Open(Location::Vault, 7)] {
        let mut life = before.clone();
        assert_eq!(life.trash_done(), None);
        assert_eq!(life, before);
    }
}

#[test]
fn a_load_during_a_trash_opens_trashing() {
    let mut life = Lifecycle::Trashing(None);
    life.open(Location::Vault, ());
    assert_eq!(life, trashing(Location::Vault));
    assert!(!life.is_open());
    assert!(life.bound().is_some());
}

#[test]
fn a_load_opens_a_closed_note() {
    let mut life = Lifecycle::Closed;
    life.open(Location::Vault, ());
    assert_eq!(life, open(Location::Vault));
    assert!(life.is_open());
}

#[test]
fn a_second_load_replaces_the_bound_note_in_its_phase() {
    let mut life = Lifecycle::Open(Location::Vault, 1);
    life.open(detached(), 2);
    assert_eq!(life, Lifecycle::Open(detached(), 2));

    let mut life = Lifecycle::Trashing(Some((Location::Vault, 1)));
    life.open(gone(), 2);
    assert_eq!(life, Lifecycle::Trashing(Some((gone(), 2))));
}

#[test]
fn claims() {
    assert_eq!(open(gone()).gone_path(), Some(&path()));
    assert_eq!(trashing(gone()).gone_path(), Some(&path()));
    for other in [
        open(Location::Vault),
        open(detached()),
        trashing(Location::Vault),
    ] {
        assert_eq!(other.gone_path(), None);
    }
    assert!(open(Location::Vault).is_open());
    assert!(open(detached()).is_open());
    for not_a_claimant in [
        trashing(gone()),
        Lifecycle::Trashing(None),
        Lifecycle::Closed,
    ] {
        assert!(!not_a_claimant.is_open());
    }
}

#[test]
fn bound_and_location() {
    let open = Lifecycle::Open(Location::Vault, 1);
    let trashing = Lifecycle::Trashing(Some((gone(), 2)));
    assert_eq!(open.bound(), Some(&1));
    assert_eq!(open.location(), Some(&Location::Vault));
    assert_eq!(trashing.bound(), Some(&2));
    assert_eq!(trashing.location(), Some(&gone()));
    for unbound in [Lifecycle::<u8>::Closed, Lifecycle::Trashing(None)] {
        assert_eq!(unbound.bound(), None);
        assert_eq!(unbound.location(), None);
        assert!(!unbound.is_open());
        assert!(!unbound.is_gone());
    }
}

#[test]
fn bound_mut_reaches_the_bound_note() {
    let mut life = Lifecycle::Open(Location::Vault, 1);
    if let Some(bound) = life.bound_mut() {
        *bound = 3;
    }
    assert_eq!(life.bound(), Some(&3));
    let mut life = Lifecycle::Trashing(Some((Location::Vault, 1)));
    if let Some(bound) = life.bound_mut() {
        *bound = 4;
    }
    assert_eq!(life.bound(), Some(&4));
    assert_eq!(Lifecycle::<u8>::Closed.bound_mut(), None);
    assert_eq!(Lifecycle::<u8>::Trashing(None).bound_mut(), None);
}

#[test]
fn a_trashing_gone_note_is_gone() {
    assert!(trashing(gone()).is_gone());
    assert!(!trashing(Location::Vault).is_gone());
    assert!(!open(detached()).is_gone());
}

#[test]
fn only_an_open_note_is_open() {
    assert!(open(Location::Vault).is_open());
    assert!(open(gone()).is_open());
    assert!(!trashing(Location::Vault).is_open());
}

#[test]
fn only_a_closed_note_is_closed() {
    assert!(Lifecycle::<()>::Closed.is_closed());
    assert!(!Lifecycle::<()>::Trashing(None).is_closed());
    assert!(!open(Location::Vault).is_closed());
}

#[test]
fn the_trash_target_of_a_gone_note() {
    let life = Lifecycle::Trashing(Some((gone(), 5)));
    assert_eq!(life.last_write(), Some((&path(), &5)));
    for other in [
        Lifecycle::Trashing(Some((Location::Vault, 5))),
        Lifecycle::Trashing(Some((detached(), 5))),
        Lifecycle::Open(gone(), 5),
        Lifecycle::Trashing(None),
        Lifecycle::Closed,
    ] {
        assert_eq!(other.last_write(), None);
    }
}

#[test]
fn only_a_gone_note_recreates_its_folder() {
    use crate::vault::session::Folder;
    assert!(matches!(gone().folder(), Folder::Create));
    assert!(matches!(Location::Vault.folder(), Folder::MustExist));
    assert!(matches!(detached().folder(), Folder::MustExist));
}

#[derive(Debug, Clone)]
enum Event {
    Open,
    Swapped(Option<u8>, Option<u8>, bool),
    BeginTrash(bool),
    TrashFailed,
    TrashDone,
}

fn event() -> impl Strategy<Value = Event> {
    prop_oneof![
        Just(Event::Open),
        (any::<Option<u8>>(), any::<Option<u8>>(), any::<bool>())
            .prop_map(|(now, before, last)| Event::Swapped(now, before, last)),
        any::<bool>().prop_map(Event::BeginTrash),
        Just(Event::TrashFailed),
        Just(Event::TrashDone),
    ]
}

proptest! {
    /// As in the store: one id loads at one kind of location, `EntryId::DETACHED` never has an entry,
    /// and failed and done follow a begun trash. Everything else comes in any order.
    #[test]
    fn no_event_sequence_reaches_a_wrong_state(
        detached_id in any::<bool>(),
        events in prop::collection::vec(event(), 0..40),
    ) {
        let mut life = Lifecycle::<()>::Closed;
        let (mut in_flight, mut done) = (false, false);
        for event in events {
            let was_gone = life.is_gone();
            match event {
                Event::Open => life.open(if detached_id { detached() } else { Location::Vault }, ()),
                Event::Swapped(now, before, last) => {
                    let swap = life.swapped(
                        now.map(|secs| mtime(secs.into())),
                        before.map(|secs| mtime(secs.into())),
                        || last.then(path),
                    );
                    prop_assert_eq!(swap.flipped, was_gone != life.is_gone());
                }
                Event::BeginTrash(has_entry) => {
                    let begun = life.begin_trash(has_entry && !detached_id).is_some();
                    prop_assert!(!(begun && (in_flight || done)));
                    in_flight |= begun;
                }
                Event::TrashFailed if in_flight => {
                    life.trash_failed();
                    in_flight = false;
                }
                Event::TrashDone if in_flight => {
                    life.trash_done();
                    (in_flight, done) = (false, true);
                }
                Event::TrashFailed | Event::TrashDone => {}
            }
            prop_assert!(!matches!(life, Lifecycle::Trashing(Some((Location::Detached(_), ())))));
            prop_assert_eq!(in_flight || done, matches!(life, Lifecycle::Trashing(_)));
            if done {
                prop_assert!(!life.is_open());
            }
        }
    }
}

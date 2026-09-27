use inkra_vault::EntryId;

use super::{SHARDS, ShardKey, Shards};
use crate::test_support::{id, vault};
use crate::words::ordinal::Ordinal;

fn numbered(n: u32) -> Shards<Ordinal, u32> {
    (0..n).map(|n| (Ordinal::new(n), n)).collect()
}

#[test]
fn insert_get_and_remove_count_every_key() {
    let mut shards = numbered(200);
    assert_eq!(shards.len(), 200);
    assert_eq!(shards.get(Ordinal::new(150)), Some(&150));
    shards.insert(Ordinal::new(150), 7);
    assert_eq!(shards.len(), 200, "a replaced value adds no key");
    assert_eq!(shards.remove(Ordinal::new(150)), Some(7));
    assert_eq!(shards.remove(Ordinal::new(150)), None);
    assert_eq!(shards.get(Ordinal::new(150)), None);
    assert_eq!(shards.len(), 199);
    let mut listed: Vec<u32> = shards.iter().map(|(_, &n)| n).collect();
    listed.sort_unstable();
    assert_eq!(listed, (0..200).filter(|&n| n != 150).collect::<Vec<_>>());
    for n in 0..200 {
        shards.remove(Ordinal::new(n));
    }
    assert!(shards.is_empty());
}

#[test]
fn a_write_under_a_held_clone_copies_one_shard() {
    let mut shards = numbered(1_000);
    let held = shards.clone();
    shards.insert(Ordinal::new(1_000), 1_000);
    assert_eq!(shards.shards_shared(&held), SHARDS - 1);
    let other = (0..1_000)
        .map(Ordinal::new)
        .find(|ordinal| ordinal.shard() != Ordinal::new(1_000).shard());
    let Some(other) = other else {
        panic!("some ordinal sits in another shard");
    };
    shards.remove(other);
    assert_eq!(shards.shards_shared(&held), SHARDS - 2);
    assert_eq!(held.len(), 1_000);
    assert_eq!(held.get(Ordinal::new(1_000)), None);
    assert!(held.get(other).is_some());
}

#[test]
fn removing_an_absent_key_copies_nothing() {
    let mut shards = numbered(100);
    let held = shards.clone();
    assert_eq!(shards.remove(Ordinal::new(100)), None);
    assert!(shards.is_shared(&held));
}

#[test]
fn entry_ids_spread_over_every_shard() {
    let files: Vec<String> = (0..10 * SHARDS).map(|n| format!("n{n}.md")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let (_fs, vault) = vault(&files);
    let ids: Vec<EntryId> = files.iter().map(|file| id(&vault, file)).collect();
    let mut per_shard = [0_usize; SHARDS];
    for &id in &ids {
        per_shard[id.shard()] += 1;
    }
    assert!(
        per_shard.iter().all(|&n| (1..=30).contains(&n)),
        "{per_shard:?}"
    );
}

#[test]
fn ordinals_fill_the_shards_in_turn() {
    for (n, at) in (0..).zip(0..3 * SHARDS) {
        assert_eq!(Ordinal::new(n).shard(), at % SHARDS);
    }
}

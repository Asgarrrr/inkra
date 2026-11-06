use std::collections::HashSet;
use std::sync::Arc;

use inkra_vault::{EntryId, Stamp};

use crate::test_support::{id, mtime, vault};
use crate::{TEXT_CACHE_BYTES, TextCache};

fn ids() -> [EntryId; 3] {
    let (_fs, vault) = vault(&["a.md", "b.md", "c.md"]);
    ["a.md", "b.md", "c.md"].map(|path| id(&vault, path))
}

fn stamp(n: u64) -> Stamp {
    Stamp {
        mtime: mtime(n),
        size: n,
    }
}

fn text(len: usize) -> Arc<str> {
    "x".repeat(len).into()
}

fn cached(cache: &mut TextCache, id: EntryId) -> bool {
    cache.get(id, stamp(0)).is_some()
}

fn pins(ids: &[EntryId]) -> HashSet<EntryId> {
    ids.iter().copied().collect()
}

#[test]
fn equal_stamp_hits() {
    let [a, ..] = ids();
    let mut cache = TextCache::new(5);
    cache.insert(a, "alpha".into(), stamp(1));

    assert_eq!(cache.get(a, stamp(1)).as_deref(), Some("alpha"));
    assert_eq!(
        cache.get(a, stamp(1)).as_deref(),
        Some("alpha"),
        "a hit keeps it"
    );
}

#[test]
fn other_stamp_misses_and_keeps() {
    let [a, ..] = ids();
    let mut cache = TextCache::new(5);
    cache.insert(a, "newer".into(), stamp(2));

    assert_eq!(cache.get(a, stamp(1)), None, "a search at the older stamp");
    assert_eq!(
        cache.get(a, stamp(2)).as_deref(),
        Some("newer"),
        "the saved text survives the miss"
    );
}

#[test]
fn insert_replaces() {
    let [a, b, _] = ids();
    let mut cache = TextCache::new(10);
    cache.insert(a, text(5), stamp(1));
    cache.insert(a, text(5), stamp(0));
    cache.insert(b, text(5), stamp(0));

    assert!(cached(&mut cache, a), "the replaced text counts once");
    assert!(cached(&mut cache, b));
}

#[test]
fn exactly_at_budget_stays() {
    let [a, b, _] = ids();
    let mut cache = TextCache::new(10);
    cache.insert(a, text(5), stamp(0));
    cache.insert(b, text(5), stamp(0));

    assert!(cached(&mut cache, a));
    assert!(cached(&mut cache, b));
}

#[test]
fn budget_plus_one_evicts_the_oldest() {
    let [a, b, _] = ids();
    let mut cache = TextCache::new(10);
    cache.insert(a, text(5), stamp(0));
    cache.insert(b, text(6), stamp(0));

    assert!(!cached(&mut cache, a));
    assert!(cached(&mut cache, b));
}

#[test]
fn get_refreshes_recency() {
    let [a, b, c] = ids();
    let mut cache = TextCache::new(10);
    cache.insert(a, text(5), stamp(0));
    cache.insert(b, text(5), stamp(0));
    assert!(cached(&mut cache, a));
    cache.insert(c, text(5), stamp(0));

    assert!(!cached(&mut cache, b), "b is the oldest read");
    assert!(cached(&mut cache, a));
    assert!(cached(&mut cache, c));
}

#[test]
fn reads_order_by_their_last_get() {
    let [a, b, c] = ids();
    let mut cache = TextCache::new(10);
    cache.insert(a, text(5), stamp(0));
    cache.insert(b, text(5), stamp(0));
    assert!(cached(&mut cache, b));
    assert!(cached(&mut cache, a));
    cache.insert(c, text(5), stamp(0));

    assert!(!cached(&mut cache, b));
    assert!(cached(&mut cache, a));
    assert!(cached(&mut cache, c));
}

#[test]
fn the_budget_is_eight_megabytes() {
    let [a, b, _] = ids();
    let mut cache = TextCache::new(TEXT_CACHE_BYTES);
    cache.insert(a, text(8 * 1024 * 1024), stamp(0));
    assert!(cached(&mut cache, a));

    cache.insert(b, text(1), stamp(0));
    assert!(!cached(&mut cache, a));
}

#[test]
fn pinned_never_evicted() {
    let [a, b, c] = ids();
    let mut cache = TextCache::new(5);
    cache.pin_only(&pins(&[a]));
    cache.insert(a, text(8), stamp(0));
    cache.insert(b, text(5), stamp(0));
    cache.insert(c, text(1), stamp(0));

    assert!(cached(&mut cache, a), "pinned, even over the budget");
    assert!(!cached(&mut cache, b));
    assert!(!cached(&mut cache, c), "a pinned text still counts");
}

#[test]
fn an_unpinned_text_over_the_budget_is_dropped() {
    let [a, ..] = ids();
    let mut cache = TextCache::new(5);
    cache.insert(a, text(6), stamp(0));

    assert!(!cached(&mut cache, a));
}

#[test]
fn pin_only_unpins_the_rest() {
    let [a, b, _] = ids();
    let mut cache = TextCache::new(5);
    cache.pin_only(&pins(&[a, b]));
    cache.insert(a, text(5), stamp(0));
    cache.insert(b, text(5), stamp(0));

    cache.pin_only(&pins(&[b]));
    assert!(!cached(&mut cache, a));
    assert!(cached(&mut cache, b));
}

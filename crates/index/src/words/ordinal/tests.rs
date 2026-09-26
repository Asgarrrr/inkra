use super::{Alive, Ordinal};

fn alive(ordinals: &[u32]) -> Alive {
    ordinals.iter().copied().map(Ordinal::new).collect()
}

fn listed(alive: &Alive) -> Vec<u32> {
    alive.iter().map(Ordinal::get).collect()
}

#[test]
fn bits_set_and_clear_across_words() {
    let mut bits = alive(&[0, 63, 64, 200]);
    assert_eq!(bits.len(), 4);
    assert_eq!(listed(&bits), [0, 63, 64, 200]);
    assert!(bits.contains(Ordinal::new(63)) && bits.contains(Ordinal::new(64)));
    assert!(!bits.contains(Ordinal::new(1)));
    assert!(!bits.contains(Ordinal::new(100_000)), "past the end");
    assert_eq!(bits.heap_bytes(), 4 * size_of::<u64>());

    bits.insert(Ordinal::new(63));
    assert_eq!(bits.len(), 4, "a set bit counts once");
    bits.remove(Ordinal::new(63));
    bits.remove(Ordinal::new(63));
    bits.remove(Ordinal::new(100_000));
    assert_eq!(
        bits.len(),
        3,
        "a clear bit and one past the end count nothing"
    );
    assert_eq!(listed(&bits), [0, 64, 200]);
}

#[test]
fn a_far_ordinal_has_a_bit_and_an_index() {
    let bits = alive(&[1_000_000]);
    assert!(bits.contains(Ordinal::new(1_000_000)));
    assert_eq!(listed(&bits), [1_000_000]);
    assert_eq!(Ordinal::new(u32::MAX).index(), 4_294_967_295);
}

#[test]
fn an_empty_set_costs_nothing() {
    let bits = Alive::default();
    assert_eq!((bits.len(), bits.heap_bytes()), (0, 0));
    assert_eq!(listed(&bits), Vec::<u32>::new());
    assert_eq!(alive(&[]), bits);
}

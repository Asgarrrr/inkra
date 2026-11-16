use super::Generics;

#[test]
fn a_list_the_look_ahead_decides_pairs_no_line() {
    for line in ["<T>(x: T) => x", "f<A, B<C>>(x)", "a < b; c > d", "a < b"] {
        let generics = Generics::new(line.as_bytes());
        generics.end(line.find('<').unwrap());
        assert!(generics.ends.get().is_none(), "{line:?}");
    }
}

#[test]
fn the_line_pairing_finds_each_list_in_its_order() {
    // Three lists close, in an order unlike their opens', past the look-ahead.
    let line = format!("f<A<B>, C<D>{}>(x)", " ".repeat(80));
    let generics = Generics::new(line.as_bytes());
    assert_eq!(generics.end(1), Some(line.len() - 3));
    assert_eq!(generics.end(3), Some(6));
    assert_eq!(generics.end(9), Some(12));
    assert!(generics.ends.get().is_some());
}

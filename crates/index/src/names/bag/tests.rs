use super::CharBag;

fn bag(text: &str) -> CharBag {
    let mut bag = CharBag::default();
    for c in text.chars() {
        bag.insert(c);
    }
    bag
}

#[test]
fn a_bag_contains_its_subsets() {
    assert!(bag("plan").contains_all(bag("pna")));
    assert!(bag("plan").contains_all(CharBag::default()));
}

#[test]
fn a_missing_letter_or_digit_is_rejected() {
    assert!(!bag("plan").contains_all(bag("x")));
    assert!(!bag("plan1").contains_all(bag("2")));
}

#[test]
fn a_lowercase_letter_or_digit_has_a_slot_of_its_own() {
    for c in ('a'..='z').chain('0'..='9') {
        for other in ('\0'..'\u{3000}').filter(|&other| other != c) {
            assert!(
                !bag(&other.to_string()).contains_all(bag(&c.to_string())),
                "{other:?} shares {c:?}'s slot"
            );
        }
    }
}

use super::*;

fn made_past_the_deadline(_: char) -> char {
    panic!("a chunk made past the deadline")
}

#[test]
fn open_colours_every_block_before_its_deadline() {
    let chunks = open_chunks(2, || ['a', 'b'], || true);

    assert_eq!(chunks, ['a', 'b']);
}

#[test]
fn open_past_its_deadline_lands_only_the_seed() {
    let chunks = open_chunks(
        2,
        || ['a', 'b'].into_iter().map(made_past_the_deadline),
        || false,
    );

    assert_eq!(chunks, []);
}

#[test]
fn a_text_over_the_parse_limit_lands_only_the_seed() {
    let chunks = open_chunks(
        OPEN_PARSE_LIMIT + 1,
        || -> [char; 1] { panic!("a text over the limit parsed") },
        || true,
    );

    assert_eq!(chunks, []);
}

#[test]
fn a_text_at_the_parse_limit_is_coloured() {
    let chunks = open_chunks(OPEN_PARSE_LIMIT, || ['a'], || true);

    assert_eq!(chunks, ['a']);
}

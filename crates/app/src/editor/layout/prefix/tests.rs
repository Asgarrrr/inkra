//! What a gutter measures (task boxes spec, D3, C): headless glyphs all advance alike, so a layout
//! test cannot tell `[x]` from `[ ]`.

use inkra_markdown::parse;

use super::measured_text;

fn measured(text: &str) -> String {
    let gutter = &parse(text).gutters[0];
    let source = &text[gutter.range.start.get()..gutter.range.end.get()];
    measured_text(source, gutter).into_owned()
}

#[test]
fn a_checked_box_measures_as_an_unchecked_one() {
    assert_eq!(measured("- [x] a"), "- [ ] ");
    assert_eq!(measured("> 1. [X] a"), "> 1. [ ] ");
    assert_eq!(measured("- [ ] a"), "- [ ] ");
    assert_eq!(measured("> a"), "> ");
}

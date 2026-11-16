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
fn every_box_state_measures_as_the_widest() {
    assert_eq!(measured("- [ ] a"), "- [X] ");
    assert_eq!(measured("- [x] a"), "- [X] ");
    assert_eq!(measured("> 1. [X] a"), "> 1. [X] ");
    assert_eq!(measured("> a"), "> ");
}

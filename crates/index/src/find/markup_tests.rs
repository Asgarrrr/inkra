use super::excerpt_tests::shown;

#[test]
fn a_window_drops_its_markers_and_keeps_its_highlights() {
    assert_eq!(
        shown("- **plan** it", "plan"),
        ("plan it".into(), vec![(0, 4)])
    );
    assert_eq!(
        shown("intro\n> We *plan*, `plans` too", "plan"),
        ("We plan, plans too".into(), vec![(3, 7), (9, 13)]),
        "lit inside emphasis and code"
    );
    assert_eq!(
        shown("# A __planned__ trip", "trip"),
        ("A planned trip".into(), vec![(10, 14)])
    );
    assert_eq!(
        shown("snake_case_plan", "snake"),
        ("snake_case_plan".into(), vec![(0, 5)])
    );
}

#[test]
fn a_window_inside_a_line_has_no_block_marker() {
    let (text, highlights) = shown(&format!("a{}- **plan**", " ".repeat(70)), "plan");
    assert_eq!(text, format!("{}- plan", " ".repeat(56)));
    assert_eq!(highlights, [(58, 62)]);
}

#[test]
fn a_matched_word_inside_a_marker_is_not_lit() {
    assert_eq!(shown("1. plan", "1"), ("plan".into(), vec![]));
    assert_eq!(shown("- [x] plan x", "x"), ("plan x".into(), vec![(5, 6)]));
}

#[test]
fn a_multibyte_window_drops_its_markers() {
    let (text, highlights) = shown("> **λόγος** ω", "λογ");
    assert_eq!(text, "λόγος ω");
    assert_eq!(&text[highlights[0].0..highlights[0].1], "λόγ");
}

#[test]
fn markup_crossing_a_window_edge_is_dropped() {
    let (text, highlights) = shown(&format!("a **{}b** then plan", "b ".repeat(39)), "plan");
    assert_eq!(text, format!("{}b then plan", &"b ".repeat(39)[27..]));
    assert_eq!(highlights, [(text.len() - 4, text.len())]);

    let (text, highlights) = shown(
        &format!("plan then **b{}** z\nnext", " b".repeat(60)),
        "plan",
    );
    assert_eq!(text, format!("plan then b{}", &" b".repeat(60)[..91]));
    assert_eq!(highlights, [(0, 4)]);
}

#[test]
fn an_underscore_inside_a_word_at_a_window_start_is_kept() {
    let text = format!("aaaa_bbbb_ {} plan", "x".repeat(52));
    let (text, highlights) = shown(&text, "plan");
    assert_eq!(text, format!("_bbbb_ {} plan", "x".repeat(52)));
    assert_eq!(highlights, [(60, 64)]);
}

#[test]
fn the_markup_is_parsed_over_its_line_alone() {
    assert_eq!(
        shown("intro\n2) **plan** it", "plan"),
        ("plan it".into(), vec![(0, 4)]),
        "the line before cannot make it a paragraph"
    );
}

#[test]
fn markup_closing_past_the_parsed_bytes_stays() {
    let (text, _) = shown(
        &format!("plan then **b{}** z\nnext", " b".repeat(600)),
        "plan",
    );
    assert_eq!(text, format!("plan then **b{}", &" b".repeat(600)[..91]));
}

#[test]
fn an_indented_line_drops_its_markup() {
    assert_eq!(
        shown("\t- **plan** it", "plan"),
        ("plan it".into(), vec![(0, 4)])
    );
    assert_eq!(
        shown("    - **plan** it", "plan"),
        ("plan it".into(), vec![(0, 4)])
    );
    let (text, highlights) = shown(&format!("\t{} **plan** it", "x".repeat(60)), "plan");
    assert_eq!(text, format!("{} plan it", "x".repeat(57)));
    assert_eq!(highlights, [(58, 62)]);
    let (text, highlights) = shown(&format!("{}- **plan**", " ".repeat(70)), "plan");
    assert_eq!(
        text,
        format!("{}plan", " ".repeat(56)),
        "a window inside the indent"
    );
    assert_eq!(highlights, [(56, 60)]);
}

#[test]
fn a_context_starting_inside_its_line_drops_inline_markup() {
    let (text, highlights) = shown(&format!("a{}**plan** it", " ".repeat(1200)), "plan");
    assert_eq!(text, format!("{}plan it", " ".repeat(58)));
    assert_eq!(highlights, [(58, 62)]);
}

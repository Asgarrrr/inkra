use super::{Markdown, parse};
use crate::HeadingLevel::{H1, H2};
use crate::{CodeBlock, HeadingLevel, Styles};

const S: Styles = Styles::STRONG;
const E: Styles = Styles::EMPHASIS;
const X: Styles = Styles::STRIKETHROUGH;
const C: Styles = Styles::CODE;
const B: Styles = Styles::CODE_BLOCK;
const M: Styles = Styles::MARKER;

/// Each span as the text it covers, so expectations read like the source.
fn styled(text: &str) -> Vec<(&str, Styles)> {
    parse(text)
        .spans
        .iter()
        .map(|span| {
            (
                &text[span.range.start.get()..span.range.end.get()],
                span.styles,
            )
        })
        .collect()
}

#[test]
fn plain_text_has_no_spans() {
    assert_eq!(styled("plain text"), vec![]);
}

#[test]
fn strong_marks_its_delimiters() {
    assert_eq!(
        styled("**a**"),
        vec![("**", S | M), ("a", S), ("**", S | M)]
    );
    assert_eq!(
        styled("__a__"),
        vec![("__", S | M), ("a", S), ("__", S | M)]
    );
}

#[test]
fn emphasis_marks_its_delimiters() {
    assert_eq!(styled("*a*"), vec![("*", E | M), ("a", E), ("*", E | M)]);
    assert_eq!(styled("_a_"), vec![("_", E | M), ("a", E), ("_", E | M)]);
}

#[test]
fn strikethrough_marks_its_delimiters() {
    assert_eq!(
        styled("~~a~~"),
        vec![("~~", X | M), ("a", X), ("~~", X | M)]
    );
}

#[test]
fn inline_code_marks_its_backtick_runs() {
    assert_eq!(styled("`a`"), vec![("`", C | M), ("a", C), ("`", C | M)]);
    assert_eq!(
        styled("``a ` b``"),
        vec![("``", C | M), ("a ` b", C), ("``", C | M)]
    );
}

#[test]
fn a_code_span_across_lines_keeps_code_off_the_next_line_s_prefix() {
    const Q: Styles = Styles::QUOTE;
    assert_eq!(
        styled("- `a\n  b`"),
        vec![
            ("- ", M),
            ("`", C | M),
            ("a", C),
            ("  ", M),
            ("b", C),
            ("`", C | M),
        ]
    );
    assert_eq!(
        styled("> `a\n> b`"),
        vec![
            ("> ", Q | M),
            ("`", Q | C | M),
            ("a", Q | C),
            ("> ", Q | M),
            ("b", Q | C),
            ("`", Q | C | M),
        ]
    );
    assert_eq!(
        styled("`a\n  b`"),
        vec![("`", C | M), ("a", C), ("  ", M), ("b", C), ("`", C | M)]
    );
}

#[test]
fn nesting_flattens_into_unions() {
    assert_eq!(
        styled("**b *i***"),
        vec![
            ("**", S | M),
            ("b ", S),
            ("*", S | E | M),
            ("i", S | E),
            ("*", S | E | M),
            ("**", S | M),
        ]
    );
    assert_eq!(
        styled("***x***"),
        vec![
            ("*", E | M),
            ("**", S | E | M),
            ("x", S | E),
            ("**", S | E | M),
            ("*", E | M),
        ]
    );
}

#[test]
fn offsets_are_bytes_across_multibyte_text() {
    assert_eq!(
        styled("héllo **wörld**"),
        vec![("**", S | M), ("wörld", S), ("**", S | M)]
    );
}

#[test]
fn an_escape_backslash_is_a_marker() {
    assert_eq!(styled("a\\*b"), vec![("\\", M)]);
}

const L: Styles = Styles::LINK;
const Q: Styles = Styles::QUOTE;
const F: Styles = Styles::FRONTMATTER;

fn h(level: HeadingLevel) -> Styles {
    Styles::heading(level)
}

#[test]
fn atx_headings_mark_their_hashes() {
    assert_eq!(styled("# T"), vec![("# ", h(H1) | M), ("T", h(H1))]);
    assert_eq!(
        styled("# T ##"),
        vec![("# ", h(H1) | M), ("T", h(H1)), (" ##", h(H1) | M)]
    );
}

#[test]
fn a_heading_stops_at_its_line_ending() {
    assert_eq!(styled("## T\nx"), vec![("## ", h(H2) | M), ("T", h(H2))]);
}

#[test]
fn a_setext_heading_marks_its_underline() {
    assert_eq!(styled("T\n==="), vec![("T", h(H1)), ("===", h(H1) | M)]);
}

#[test]
fn a_heading_carries_its_inline_styles() {
    assert_eq!(
        styled("# **b**"),
        vec![
            ("# ", h(H1) | M),
            ("**", h(H1) | S | M),
            ("b", h(H1) | S),
            ("**", h(H1) | S | M),
        ]
    );
}

#[test]
fn links_mark_everything_but_their_text() {
    assert_eq!(
        styled("[t](http://u)"),
        vec![("[", L | M), ("t", L), ("](http://u)", L | M)]
    );
    assert_eq!(
        styled("[t][r]\n\n[r]: http://x"),
        vec![("[", L | M), ("t", L), ("][r]", L | M)]
    );
    assert_eq!(
        styled("<http://a>"),
        vec![("<", L | M), ("http://a", L), (">", L | M)]
    );
    assert_eq!(
        styled("![alt](u)"),
        vec![("![", L | M), ("alt", L), ("](u)", L | M)]
    );
}

#[test]
fn wikilinks_show_the_alias_or_the_target() {
    assert_eq!(
        styled("[[Page]]"),
        vec![("[[", L | M), ("Page", L), ("]]", L | M)]
    );
    assert_eq!(
        styled("[[Page|alias]]"),
        vec![("[[Page|", L | M), ("alias", L), ("]]", L | M)]
    );
}

#[test]
fn every_quote_prefix_is_a_marker() {
    assert_eq!(
        styled("> a\n> b"),
        vec![("> ", Q | M), ("a", Q), ("> ", Q | M), ("b", Q)]
    );
    assert_eq!(
        styled("> - a\n> - b"),
        vec![("> - ", Q | M), ("a", Q), ("> - ", Q | M), ("b", Q)]
    );
}

#[test]
fn a_fence_inside_a_quote_keeps_both_styles() {
    assert_eq!(
        styled("> ```\n> code\n> ```"),
        vec![
            ("> ", Q | M),
            ("```", C | B | Q | M),
            ("> ", Q | M),
            ("code", C | B | Q),
            ("> ", Q | M),
            ("```", C | B | Q | M),
        ]
    );
}

#[test]
fn list_markers_are_dimmed() {
    assert_eq!(styled("- a\n- b"), vec![("- ", M), ("- ", M)]);
    assert_eq!(styled("1. a\n2) b"), vec![("1. ", M), ("2) ", M)]);
    assert_eq!(
        styled("- [ ] t\n- [x] u"),
        vec![("- [ ] ", M), ("- [x] ", M)]
    );
}

#[test]
fn code_blocks_mark_their_fences() {
    assert_eq!(
        styled("```rs\nlet x;\n```"),
        vec![("```rs", C | B | M), ("let x;", C | B), ("```", C | B | M)]
    );
    assert_eq!(styled("    code"), vec![("code", C | B)]);
}

#[test]
fn frontmatter_marks_its_fences() {
    assert_eq!(
        styled("---\ntitle: x\n---\nbody"),
        vec![("---", F | M), ("title: x", F), ("---", F | M)]
    );
}

#[test]
fn a_thematic_break_is_a_marker() {
    assert_eq!(styled("a\n\n---\n\nb"), vec![("---", M)]);
}

#[test]
fn a_thematic_break_keeps_its_enclosing_tags() {
    assert_eq!(
        styled("> ---\n> a"),
        vec![("> ---", Q | M), ("> ", Q | M), ("a", Q)]
    );
    assert_eq!(styled("- > ---"), vec![("- ", M), ("> ---", Q | M)]);
}

#[test]
fn table_pipes_are_markers() {
    assert_eq!(
        styled("| a | b |\n|---|---|\n| 1 | 2 |"),
        vec![
            ("| ", M),
            (" | ", M),
            (" |", M),
            ("|---|---|", M),
            ("| ", M),
            (" | ", M),
            (" |", M),
        ]
    );
}

use inkra_text::ByteOffset;

use crate::Conceal;

/// Each conceal as (marker text, element text, replacement).
fn concealed(text: &str) -> Vec<(&str, &str, &'static str)> {
    parse(text)
        .conceals
        .iter()
        .map(
            |Conceal {
                 element,
                 marker,
                 replacement,
             }| {
                (
                    &text[marker.start.get()..marker.end.get()],
                    &text[element.start.get()..element.end.get()],
                    *replacement,
                )
            },
        )
        .collect()
}

#[test]
fn plain_text_conceals_nothing() {
    assert_eq!(concealed("plain"), vec![]);
}

#[test]
fn inline_delimiters_conceal_within_their_element() {
    assert_eq!(
        concealed("**a**"),
        vec![("**", "**a**", ""), ("**", "**a**", "")]
    );
    assert_eq!(concealed("*a*"), vec![("*", "*a*", ""), ("*", "*a*", "")]);
    assert_eq!(
        concealed("~~a~~"),
        vec![("~~", "~~a~~", ""), ("~~", "~~a~~", "")]
    );
    assert_eq!(concealed("`a`"), vec![("`", "`a`", ""), ("`", "`a`", "")]);
    assert_eq!(
        concealed("``a ` b``"),
        vec![("``", "``a ` b``", ""), ("``", "``a ` b``", "")]
    );
}

#[test]
fn nested_delimiters_belong_to_the_innermost_element() {
    assert_eq!(
        concealed("**b *i***"),
        vec![
            ("**", "**b *i***", ""),
            ("*", "*i*", ""),
            ("*", "*i*", ""),
            ("**", "**b *i***", ""),
        ]
    );
    assert_eq!(
        concealed("***x***"),
        vec![
            ("*", "***x***", ""),
            ("**", "**x**", ""),
            ("**", "**x**", ""),
            ("*", "***x***", ""),
        ]
    );
}

#[test]
fn atx_headings_conceal_their_hashes_but_setext_does_not() {
    assert_eq!(concealed("# T"), vec![("# ", "# T", "")]);
    assert_eq!(concealed("# T ##"), vec![("# ", "# T ##", "")]);
    assert_eq!(concealed("## T\nx"), vec![("## ", "## T", "")]);
    assert_eq!(concealed("T\n==="), vec![]);
    assert_eq!(
        concealed("# **b**"),
        vec![
            ("# ", "# **b**", ""),
            ("**", "**b**", ""),
            ("**", "**b**", "")
        ]
    );
}

#[test]
fn a_heading_hides_its_opening_sequence_only() {
    assert_eq!(styled("# #"), vec![("# #", h(H1) | M)]);
    assert_eq!(concealed("# #"), vec![("# ", "# #", "")]);
    assert_eq!(
        styled("# a\\*b"),
        vec![
            ("# ", h(H1) | M),
            ("a", h(H1)),
            ("\\", h(H1) | M),
            ("*b", h(H1))
        ]
    );
    assert_eq!(concealed("# a\\*b"), vec![("# ", "# a\\*b", "")]);
}

#[test]
fn a_heading_inside_a_container_conceals_its_opening_sequence() {
    assert_eq!(
        concealed("> # q\n"),
        vec![(">", ">", ""), ("# ", "# q", "")]
    );
    assert_eq!(
        concealed("- # a\n"),
        vec![("-", "-", "•"), ("# ", "# a", "")]
    );
    assert_eq!(concealed("  # ind"), vec![("# ", "# ind", "")]);
}

#[test]
fn a_link_in_a_heading_keeps_its_own_conceals() {
    assert_eq!(
        concealed("# [t](u) ##"),
        vec![
            ("# ", "# [t](u) ##", ""),
            ("[", "[t](u)", ""),
            ("](u)", "[t](u)", "")
        ]
    );
}

#[test]
fn a_setext_heading_starting_with_a_hash_conceals_nothing() {
    assert_eq!(concealed("#a\n==="), vec![]);
}

#[test]
fn an_indented_atx_heading_starts_at_its_hashes() {
    let conceals = parse("   # T").conceals;
    assert_eq!(
        conceals,
        vec![Conceal {
            element: ByteOffset::new(3)..ByteOffset::new(6),
            marker: ByteOffset::new(3)..ByteOffset::new(5),
            replacement: "",
        }]
    );
}

#[test]
fn links_conceal_all_but_their_text() {
    assert_eq!(
        concealed("[t](http://u)"),
        vec![
            ("[", "[t](http://u)", ""),
            ("](http://u)", "[t](http://u)", "")
        ]
    );
    assert_eq!(
        concealed("[t][r]\n\n[r]: http://x"),
        vec![("[", "[t][r]", ""), ("][r]", "[t][r]", "")]
    );
    assert_eq!(
        concealed("<http://a>"),
        vec![("<", "<http://a>", ""), (">", "<http://a>", "")]
    );
    assert_eq!(
        concealed("[[Page]]"),
        vec![("[[", "[[Page]]", ""), ("]]", "[[Page]]", "")]
    );
    assert_eq!(
        concealed("[[Page|alias]]"),
        vec![
            ("[[Page|", "[[Page|alias]]", ""),
            ("]]", "[[Page|alias]]", "")
        ]
    );
    assert_eq!(
        concealed("**[t](u)**"),
        vec![
            ("**", "**[t](u)**", ""),
            ("[", "[t](u)", ""),
            ("](u)", "[t](u)", ""),
            ("**", "**[t](u)**", ""),
        ]
    );
}

#[test]
fn a_link_across_lines_conceals_each_marker_on_its_own_line() {
    assert_eq!(
        concealed("[a\nb](u)"),
        vec![("[", "[a\nb](u)", ""), ("](u)", "[a\nb](u)", "")]
    );
}

#[test]
fn a_continuation_prefix_inside_a_link_is_not_concealed() {
    assert_eq!(
        concealed("> [a\n> b](u)"),
        vec![
            (">", ">", ""),
            ("[", "[a\n> b](u)", ""),
            (">", ">", ""),
            ("](u)", "[a\n> b](u)", "")
        ]
    );
}

#[test]
fn a_marker_after_a_continuation_prefix_is_concealed() {
    assert_eq!(
        concealed("> [a\n> ](u)"),
        vec![
            (">", ">", ""),
            ("[", "[a\n> ](u)", ""),
            (">", ">", ""),
            ("](u)", "[a\n> ](u)", "")
        ]
    );
}

#[test]
fn a_marker_on_a_link_continuation_line_is_concealed() {
    assert_eq!(
        concealed("[a\n](u)"),
        vec![("[", "[a\n](u)", ""), ("](u)", "[a\n](u)", "")]
    );
}

#[test]
fn images_stay_visible() {
    assert_eq!(concealed("![alt](u)"), vec![]);
}

#[test]
fn unordered_bullets_become_dots_and_numbers_stay() {
    assert_eq!(
        concealed("- a\n- b"),
        vec![("-", "-", "•"), ("-", "-", "•")]
    );
    assert_eq!(concealed("* a"), vec![("*", "*", "•")]);
    assert_eq!(concealed("1. a"), vec![]);
    assert_eq!(concealed("- [ ] t"), vec![("-", "-", "•")]);
    assert_eq!(concealed("> - a"), vec![(">", ">", ""), ("-", "-", "•")]);
    assert_eq!(
        concealed("- a\n  - b"),
        vec![("-", "-", "•"), ("-", "-", "•")]
    );
}

#[test]
fn an_indented_bullet_is_found_past_the_indentation() {
    assert_eq!(concealed("  - a"), vec![("-", "-", "•")]);
    assert_eq!(concealed("   * a"), vec![("*", "*", "•")]);
}

#[test]
fn escapes_conceal_nothing_and_a_quote_only_its_markers() {
    assert_eq!(
        concealed("> **q**"),
        vec![(">", ">", ""), ("**", "**q**", ""), ("**", "**q**", "")]
    );
    assert_eq!(concealed("a\\*b"), vec![]);
}

#[test]
fn conceal_offsets_are_bytes_across_multibyte_text() {
    assert_eq!(
        concealed("héllo **wörld**"),
        vec![("**", "**wörld**", ""), ("**", "**wörld**", "")]
    );
}

use proptest::prelude::*;

/// A language is one word on the block's first line.
fn check_language(text: &str, block: &CodeBlock) {
    let Some(language) = &block.language else {
        return;
    };
    let (start, end) = (block.range.start.get(), block.range.end.get());
    let (from, to) = (language.start.get(), language.end.get());
    assert!(from < to, "empty language in {block:?}");
    assert!(start <= from && to <= end, "language outside {block:?}");
    assert!(text.is_char_boundary(from) && text.is_char_boundary(to));
    assert!(
        !text[start..to].contains('\n'),
        "language off the first line of {block:?} in {text:?}"
    );
    assert!(
        !text[from..to]
            .bytes()
            .any(|byte| byte.is_ascii_whitespace()),
        "language holds whitespace in {block:?} in {text:?}"
    );
}

fn check_invariants(text: &str) {
    let Markdown {
        spans,
        conceals,
        code_blocks,
        ..
    } = parse(text);
    for span in &spans {
        let (start, end) = (span.range.start.get(), span.range.end.get());
        assert!(start < end, "empty span {span:?} in {text:?}");
        assert!(end <= text.len(), "span {span:?} past the end of {text:?}");
        assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        assert!(
            !text[start..end].contains('\n'),
            "span {span:?} crosses a line"
        );
        assert!(!span.styles.is_empty());
    }
    for pair in spans.windows(2) {
        assert!(
            pair[0].range.end <= pair[1].range.start,
            "unsorted or overlapping: {pair:?}"
        );
        if pair[0].range.end == pair[1].range.start {
            assert_ne!(
                pair[0].styles, pair[1].styles,
                "unmerged neighbours: {pair:?}"
            );
        }
    }
    for conceal in &conceals {
        let (start, end) = (conceal.marker.start.get(), conceal.marker.end.get());
        assert!(start < end, "empty marker {conceal:?} in {text:?}");
        assert!(
            end <= text.len(),
            "marker {conceal:?} past the end of {text:?}"
        );
        assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        assert!(
            conceal.element.start <= conceal.marker.start
                && conceal.marker.end <= conceal.element.end,
            "marker outside its element: {conceal:?} in {text:?}"
        );
        assert!(
            !text[start..end].contains('\n'),
            "marker {conceal:?} crosses a line in {text:?}"
        );
        let element_end = conceal.element.end.get();
        assert!(
            !text[conceal.element.start.get()..element_end].ends_with('\n')
                || text[element_end..].is_empty()
                || text[element_end..].starts_with('\n'),
            "element {conceal:?} keeps its line ending in {text:?}"
        );
        if conceal.replacement == "•" {
            assert!(
                ["-", "*", "+"].contains(&&text[start..end]),
                "a dot replaces a non-bullet: {conceal:?} in {text:?}"
            );
        }
    }
    for pair in conceals.windows(2) {
        assert!(
            pair[0].marker.end <= pair[1].marker.start,
            "unsorted or overlapping markers: {pair:?} in {text:?}"
        );
    }
    for block in &code_blocks {
        let (start, end) = (block.range.start.get(), block.range.end.get());
        assert!(start < end, "empty block {block:?} in {text:?}");
        assert!(
            end <= text.len(),
            "block {block:?} past the end of {text:?}"
        );
        assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
        assert!(
            text[end..].is_empty() || text[end..].starts_with('\n'),
            "block {block:?} keeps its line ending in {text:?}"
        );
        check_language(text, block);
    }
    for pair in code_blocks.windows(2) {
        assert!(
            pair[0].range.end < pair[1].range.start,
            "unsorted or touching blocks: {pair:?} in {text:?}"
        );
    }
    for span in spans.iter().filter(|span| span.styles.contains(B)) {
        assert!(
            code_blocks
                .iter()
                .any(|block| block.range.start <= span.range.start
                    && span.range.end <= block.range.end),
            "block span {span:?} outside every block in {text:?}"
        );
    }
}

#[test]
fn an_empty_wikilink_alias_is_all_markers() {
    check_invariants("[[0|]]\n [");
    check_invariants(">[[(|]]0");
    assert_eq!(styled("[[0|]]\n ["), vec![("[[0|]]", L | M), (" ", M)]);
    assert_eq!(
        styled(">[[(|]]0"),
        vec![(">", Q | M), ("[[(|]]", Q | L | M), ("0", Q)]
    );
    check_invariants("[[_|]]~*0*");
    assert_eq!(
        styled("[[_|]]~*0*"),
        vec![("[[_|]]", L | M), ("*", E | M), ("0", E), ("*", E | M)]
    );
    assert_eq!(
        concealed("[[_|]]~*0*"),
        vec![
            ("[[_|", "[[_|]]", ""),
            ("]]", "[[_|]]", ""),
            ("*", "*0*", ""),
            ("*", "*0*", "")
        ]
    );
}

/// pulldown-cmark 0.13.4 repeated the events after each empty alias, exponentially (RUSTSEC-2026-0414).
#[test]
fn many_empty_wikilink_aliases_give_one_span_each() {
    let text = "[[a|]] ".repeat(16);
    let mut expected = vec![("[[a|]]", L | M); 16];
    expected.push((" ", M));
    assert_eq!(styled(&text), expected);
    assert_eq!(parse(&text).conceals.len(), 32);
}

#[test]
fn an_empty_wikilink_alias_leaves_its_containers_open() {
    assert_eq!(
        styled("> [[_|]]*0*\n>\n> b"),
        vec![
            ("> ", Q | M),
            ("[[_|]]", Q | L | M),
            ("*", Q | E | M),
            ("0", Q | E),
            ("*", Q | E | M),
            (">", Q | M),
            ("> ", Q | M),
            ("b", Q),
        ]
    );
}

fn syntax_fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("```\n".to_string()),
        Just("```rs\n".to_string()),
        Just("~~~\n".to_string()),
        Just("# ".to_string()),
        Just("> ".to_string()),
        Just(">".to_string()),
        Just("    ".to_string()),
        Just("- ".to_string()),
        Just("  - ".to_string()),
        Just("1. ".to_string()),
        Just("---\n".to_string()),
        Just("| a | b |\n|---|---|\n".to_string()),
        Just("**".to_string()),
        Just("*".to_string()),
        Just("`".to_string()),
        Just("[t](u)".to_string()),
        Just("[[p|a]]".to_string()),
        "[a-zé]{1,8}".prop_map(String::from),
        Just("\n".to_string()),
    ]
}

proptest! {
    #[test]
    fn spans_hold_their_invariants_on_any_text(text in any::<String>()) {
        check_invariants(&text);
    }

    #[test]
    fn spans_hold_their_invariants_on_syntax_heavy_text(
        text in "[a-zé *_`\\[\\]()#>!|~:=+.0-9\n-]{0,120}"
    ) {
        check_invariants(&text);
    }

    #[test]
    fn spans_hold_their_invariants_on_block_constructs(
        fragments in prop::collection::vec(syntax_fragment(), 0..40)
    ) {
        check_invariants(&fragments.concat());
    }

}

mod code_blocks;
mod gutters;
mod headings;

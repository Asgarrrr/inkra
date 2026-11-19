//! `Markdown::gutters` (markdown blocks spec, D1): one per quoted line, found from the parse.

use proptest::prelude::*;

use super::super::parse;
use super::{concealed, styled, syntax_fragment};
use crate::{Conceal, Gutter, ItemPart, Styles};

/// Each gutter as its text, its `>` offsets and its depth.
fn gutters(text: &str) -> Vec<(&str, Vec<usize>, usize)> {
    parse(text)
        .gutters
        .iter()
        .map(|gutter| {
            let range = gutter.range.start.get()..gutter.range.end.get();
            let quotes = gutter.quotes.iter().map(|quote| quote.get()).collect();
            (&text[range], quotes, gutter.depth)
        })
        .collect()
}

#[test]
fn every_quoted_line_has_a_gutter() {
    assert_eq!(
        gutters("> a\n> b"),
        vec![("> ", vec![0], 1), ("> ", vec![4], 1)]
    );
    assert_eq!(gutters("a\n> b"), vec![("> ", vec![2], 1)]);
    assert_eq!(gutters("plain\n> > b").len(), 1);
}

#[test]
fn nested_quotes_take_one_marker_per_level() {
    assert_eq!(gutters("> > x"), vec![("> > ", vec![0, 2], 2)]);
    assert_eq!(
        gutters(">>x\n> > y"),
        vec![(">>", vec![0, 1], 2), ("> > ", vec![4, 6], 2)]
    );
    assert_eq!(
        gutters("> a\n> > b"),
        vec![("> ", vec![0], 1), ("> > ", vec![4, 6], 2)]
    );
}

#[test]
fn a_lazy_line_has_an_empty_gutter_at_its_depth() {
    assert_eq!(gutters("> a\nb"), vec![("> ", vec![0], 1), ("", vec![], 1)]);
    assert_eq!(
        gutters("> a\n> > b\nc"),
        vec![("> ", vec![0], 1), ("> > ", vec![4, 6], 2), ("", vec![], 2)]
    );
}

#[test]
fn a_line_short_of_its_depth_keeps_the_markers_it_has() {
    assert_eq!(
        gutters("> > a\n> b"),
        vec![("> > ", vec![0, 2], 2), ("> ", vec![6], 2)]
    );
}

#[test]
fn up_to_three_spaces_lead_a_marker_and_a_fourth_makes_it_text() {
    assert_eq!(
        gutters("  > a\n  > b"),
        vec![("  > ", vec![2], 1), ("  > ", vec![8], 1)]
    );
    assert_eq!(
        gutters("> a\n    > b"),
        vec![("> ", vec![0], 1), ("", vec![], 1)]
    );
}

#[test]
fn one_space_or_tab_after_a_marker_joins_the_gutter() {
    assert_eq!(gutters(">\tx"), vec![(">\t", vec![0], 1)]);
    assert_eq!(gutters(">  a"), vec![("> ", vec![0], 1)]);
    assert_eq!(gutters(">a"), vec![(">", vec![0], 1)]);
}

/// A tab counts to the next tab stop and a `>` takes up to three columns before it, as
/// pulldown-cmark 0.13.6 reads them: each `>` here is the one it parses as a marker (D1).
#[test]
fn a_tab_counts_its_columns_before_and_after_a_marker() {
    let tab_leads = "> a\n\t> b";
    assert_eq!(gutters(tab_leads)[1], ("\t>", vec![5], 1));
    let tab_and_space = "> a\n\t > b";
    assert_eq!(gutters(tab_and_space)[1], ("", vec![], 1));
    assert_eq!(gutters(">\t> b"), vec![(">\t> ", vec![0, 2], 2)]);
    let four_columns = ">\t> a\n>\t  > b";
    assert_eq!(gutters(four_columns)[1], (">\t", vec![6], 2));
    let short_tab = "> > a\n\t>  \t> b";
    assert_eq!(gutters(short_tab)[1], ("\t>  \t> ", vec![7, 11], 2));
}

#[test]
fn a_fence_in_a_quote_takes_one_marker_per_quote() {
    assert_eq!(
        gutters("> ```\n> > x\n> ```"),
        vec![("> ", vec![0], 1), ("> ", vec![6], 1), ("> ", vec![12], 1)]
    );
    assert_eq!(gutters("> ```\nx"), vec![("> ", vec![0], 1)]);
}

#[test]
fn an_indented_code_block_keeps_its_indent_out_of_the_gutter() {
    assert_eq!(
        gutters(">     code\n>     more"),
        vec![("> ", vec![0], 1), ("> ", vec![11], 1)]
    );
}

#[test]
fn a_quote_inside_a_list_item_has_no_quote_part() {
    assert_eq!(
        gutters("- > a\n  > b"),
        vec![("- ", vec![], 0), ("  ", vec![], 0)]
    );
    assert_eq!(gutters("> - > a"), vec![("> - ", vec![0], 1)]);
    assert_eq!(
        gutters("> - a\n>   b"),
        vec![("> - ", vec![0], 1), (">   ", vec![6], 1)]
    );
}

#[test]
fn a_blank_quote_line_has_a_gutter_and_a_blank_line_ends_the_quote() {
    assert_eq!(
        gutters(">a\n>\n> b"),
        vec![(">", vec![0], 1), (">", vec![3], 1), ("> ", vec![5], 1)]
    );
    assert_eq!(
        gutters("> a\n\n> b"),
        vec![("> ", vec![0], 1), ("> ", vec![5], 1)]
    );
    assert_eq!(gutters("> a\n"), vec![("> ", vec![0], 1)]);
}

#[test]
fn a_heading_in_a_quote_starts_after_the_gutter() {
    assert_eq!(gutters("> # q"), vec![("> ", vec![0], 1)]);
}

/// A gutter's bytes are styled by the gutter alone, whatever inline element runs across its line.
#[test]
fn a_gutter_is_a_quote_marker_inside_any_inline_element() {
    const Q: Styles = Styles::QUOTE;
    const M: Styles = Styles::MARKER;
    const C: Styles = Styles::CODE;
    const S: Styles = Styles::STRONG;
    const L: Styles = Styles::LINK;
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
        styled("> <span\n> x>"),
        vec![("> ", Q | M), ("<span", Q), ("> ", Q | M), ("x>", Q)]
    );
    assert_eq!(
        styled("> **a\n> b**"),
        vec![
            ("> ", Q | M),
            ("**", Q | S | M),
            ("a", Q | S),
            ("> ", Q | M),
            ("b", Q | S),
            ("**", Q | S | M),
        ]
    );
    assert_eq!(
        styled("> [a\n> b](u)"),
        vec![
            ("> ", Q | M),
            ("[", Q | L | M),
            ("a", Q | L),
            ("> ", Q | M),
            ("b", Q | L),
            ("](u)", Q | L | M),
        ]
    );
}

#[test]
fn a_lines_markers_conceal_together() {
    assert_eq!(concealed("> a"), vec![(">", ">", "")]);
    assert_eq!(concealed("> > x"), vec![(">", "> >", ""), (">", "> >", "")]);
    assert_eq!(concealed("  > a"), vec![(">", "  >", "")]);
    assert_eq!(concealed("> a\nb"), vec![(">", ">", "")]);
    assert_eq!(concealed("- > a"), vec![("-", "-", "")]);
}

/// The gap rule's prefix skip consumes a superset of a gutter, so its markers start at or past the gutter's end.
#[test]
fn a_strong_across_quote_lines_keeps_its_markers_out_of_the_gutter() {
    assert_eq!(
        concealed("> **a\n> b**"),
        vec![
            (">", ">", ""),
            ("**", "**a\n> b**", ""),
            (">", ">", ""),
            ("**", "**a\n> b**", ""),
        ]
    );
}

proptest! {
    #[test]
    fn gutters_are_well_formed_on_any_text(text in any::<String>()) {
        gutters_are_well_formed(&text)?;
    }

    #[test]
    fn gutters_are_well_formed_on_block_constructs(
        fragments in prop::collection::vec(syntax_fragment(), 0..40)
    ) {
        gutters_are_well_formed(&fragments.concat())?;
    }
}

/// G1–G4.
fn gutters_are_well_formed(text: &str) -> Result<(), TestCaseError> {
    let markdown = parse(text);
    let mut previous = None;
    for gutter in &markdown.gutters {
        let (start, end) = (gutter.range.start.get(), gutter.range.end.get());
        let line_end = text[start..]
            .find('\n')
            .map_or(text.len(), |len| start + len);
        prop_assert!(
            start == 0 || text.as_bytes()[start - 1] == b'\n',
            "{gutter:?} in {text:?}"
        );
        prop_assert!(
            previous.is_none_or(|previous| previous < start),
            "{gutter:?} in {text:?}"
        );
        prop_assert!(start <= end && end <= line_end, "{gutter:?} in {text:?}");
        prop_assert!(
            1 <= gutter.depth || gutter.item.is_some(),
            "{gutter:?} in {text:?}"
        );
        prop_assert!(gutter.quotes.len() <= gutter.depth);
        prop_assert_eq!(
            gutter.quotes.is_empty() && gutter.quote_end() == gutter.range.end,
            start == end
        );
        for quote in &gutter.quotes {
            prop_assert!(gutter.range.contains(quote));
            prop_assert_eq!(text.as_bytes()[quote.get()], b'>');
            let marker = markdown.spans.iter().any(|span| {
                span.range.contains(quote) && span.styles.contains(Styles::QUOTE | Styles::MARKER)
            });
            prop_assert!(marker, "{quote:?} is no quote marker in {text:?}");
        }
        let mut inside: Vec<_> = markdown
            .conceals
            .iter()
            .filter(|conceal| gutter.range.contains(&conceal.marker.start))
            .collect();
        if let Some(item) = &gutter.item {
            let own = inside.pop_if(|conceal| conceal.marker.start == item.start);
            item_is_well_formed(text, gutter, item, own)?;
        }
        for (conceal, quote) in inside.iter().zip(&gutter.quotes) {
            prop_assert_eq!(conceal.marker.clone(), *quote..*quote + 1);
        }
        prop_assert_eq!(inside.len(), gutter.quotes.len());
        previous = Some(start);
    }
    Ok(())
}

/// G5: an item part follows its line's `>` and the quote part's blank, and holds no `>`. On an
/// item's other line it is blanks only, with no conceal: blanks show nothing to reveal. On a marker
/// line, which holds all its `>`, it ends at most one space or tab past its last mark or checkbox,
/// and its conceal hides it through that mark (D1, D2, D11).
fn item_is_well_formed(
    text: &str,
    gutter: &Gutter,
    item: &ItemPart,
    conceal: Option<&Conceal>,
) -> Result<(), TestCaseError> {
    let quote_end = gutter
        .quotes
        .last()
        .map_or(gutter.range.start.get(), |q| q.get() + 1);
    let blank = &text[quote_end..item.start.get()];
    prop_assert!(["", " ", "\t"].contains(&blank), "{gutter:?} in {text:?}");
    let part = &text[item.start.get()..gutter.range.end.get()];
    prop_assert!(!part.contains(['>', '\n']), "{gutter:?} in {text:?}");
    let marked = part.trim_end_matches([' ', '\t']);
    if marked.is_empty() {
        prop_assert!(item.task.is_none(), "{gutter:?} in {text:?}");
        prop_assert!(conceal.is_none(), "{gutter:?} in {text:?}");
        return Ok(());
    }
    prop_assert_eq!(gutter.quotes.len(), gutter.depth);
    prop_assert!(
        ["", " ", "\t"].contains(&&part[marked.len()..]),
        "{gutter:?} in {text:?}"
    );
    if let Some(task) = &item.task {
        let (start, end) = (task.checkbox.start.get(), task.checkbox.end.get());
        let checkbox: &[&str] = if task.checked {
            &["[x]", "[X]"]
        } else {
            &["[ ]"]
        };
        prop_assert!(
            checkbox.contains(&&text[start..end]),
            "{task:?} in {text:?}"
        );
        prop_assert_eq!(end, item.start.get() + marked.len());
    }
    let conceal = conceal.ok_or_else(|| TestCaseError::fail("no item conceal"))?;
    let end = item.start + marked.len();
    prop_assert_eq!(&conceal.element, &(item.start..end));
    prop_assert_eq!(&conceal.marker, &(item.start..end));
    prop_assert_eq!(conceal.replacement, "");
    Ok(())
}

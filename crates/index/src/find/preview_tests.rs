use std::sync::atomic::{AtomicBool, Ordering};

use super::excerpt_tests::{found, with_texts};
use super::preview;
use crate::test_support::id;
use crate::{Excerpt, Group, NoteQuery, excerpt};

#[test]
fn a_name_row_shows_its_first_prose_line_unlit() {
    let text = "\n  \n\t# The plan, its plan  \nbody plan";
    let (vault, index) = with_texts(&[("plan.md", Some(text))]);
    let snapshot = index.snapshot();
    let cancel = AtomicBool::new(false);
    let named = NoteQuery::new("plan", None, &[]);
    let found = found(&index, &named);
    assert_eq!(found.group, Group::Name);
    let preview = Some(Excerpt {
        text: "The plan, its plan".into(),
        highlights: Vec::new(),
    });
    assert_eq!(excerpt(&snapshot, &found, &named, text, &cancel), preview);

    let recent = NoteQuery::new("", None, &[id(&vault, "plan.md")]);
    let found = self::found(&index, &recent);
    assert_eq!(
        excerpt(&snapshot, &found, &recent, text, &cancel),
        preview,
        "an empty query"
    );
}

#[test]
fn a_huge_first_line_gives_a_160_byte_preview() {
    let text = format!("\n  abc{}", "é ".repeat(3_500_000));
    let (_vault, index) = with_texts(&[("plan.md", Some(&text))]);
    let query = NoteQuery::new("plan", None, &[]);
    let Some(preview) = excerpt(
        &index.snapshot(),
        &found(&index, &query),
        &query,
        &text,
        &AtomicBool::new(false),
    ) else {
        panic!("a preview");
    };
    assert_eq!(
        preview.text,
        format!("abc{}", "é ".repeat(52)),
        "160 bytes, cut on a char"
    );
}

#[test]
fn a_preview_past_the_scan_limit_is_none() {
    let preview = |text: &str| {
        let (_vault, index) = with_texts(&[("plan.md", Some(text))]);
        let query = NoteQuery::new("plan", None, &[]);
        excerpt(
            &index.snapshot(),
            &found(&index, &query),
            &query,
            text,
            &AtomicBool::new(false),
        )
        .map(|preview| preview.text)
    };
    assert_eq!(preview(&format!("{}title", "\n".repeat(10 << 20))), None);
    assert_eq!(
        preview(&format!("{}title", "\n \t".repeat(100_000))),
        Some("title".into())
    );
    assert_eq!(preview(" \n "), None, "a blank note");
}

fn preview_of(file: &str, text: &str, query: &str) -> Option<String> {
    let (_vault, index) = with_texts(&[(file, Some(text))]);
    let query = NoteQuery::new(query, None, &[]);
    let found = found(&index, &query);
    assert_eq!(found.group, Group::Name);
    let cancel = AtomicBool::new(false);
    let excerpt = excerpt(&index.snapshot(), &found, &query, text, &cancel)?;
    assert!(excerpt.highlights.is_empty(), "a preview is unlit");
    Some(excerpt.text)
}

fn preview(text: &str) -> Option<String> {
    preview_of("plan.md", text, "plan")
}

#[test]
fn a_heading_naming_the_note_gives_way_to_the_first_prose_line() {
    assert_eq!(
        preview_of("Plan.md", "# Plan\n\nThe first prose line.", "plan"),
        Some("The first prose line.".into())
    );
    assert_eq!(
        preview("## PLÂN !\nBody."),
        Some("Body.".into()),
        "the stem's words, folded: the exact-name rule"
    );
    assert_eq!(
        preview_of("projects/plan.md", "# Plan\nBody.", "plan"),
        Some("Body.".into()),
        "the stem alone"
    );
    assert_eq!(
        preview("# __Plan__\nBody."),
        Some("Body.".into()),
        "compared without its markers"
    );
}

#[test]
fn a_heading_not_naming_the_note_is_shown_stripped() {
    assert_eq!(preview("# Other\n\nBody."), Some("Other".into()));
    assert_eq!(preview("# The plan\nBody."), Some("The plan".into()));
    assert_eq!(preview("# Plan plan\nBody."), Some("Plan plan".into()));
    assert_eq!(
        preview_of("projects/plan.md", "# Projects plan\nBody.", "plan"),
        Some("Projects plan".into()),
        "the folders do not count"
    );
}

#[test]
fn only_the_leading_heading_gives_way() {
    assert_eq!(preview("Intro.\n# Plan"), Some("Intro.".into()));
    assert_eq!(
        preview("Plan\n\nBody."),
        Some("Plan".into()),
        "a paragraph naming the note is prose"
    );
}

#[test]
fn front_matter_is_skipped() {
    assert_eq!(
        preview("---\ntitle: Plan\ntags: [a]\n---\n# Plan\n\nBody."),
        Some("Body.".into())
    );
    assert_eq!(
        preview("---\ntitle: Plan\n---\n\nBody."),
        Some("Body.".into())
    );
    assert_eq!(
        preview("\n---\ntitle: Plan\n---\nBody."),
        Some("title: Plan".into()),
        "front matter starts on the first line"
    );
    assert_eq!(
        preview("---\ntitle: Plan\n"),
        Some("title: Plan".into()),
        "unclosed: a thematic break, then prose"
    );
    assert_eq!(
        preview("---\ntitle: Plan\n---\n"),
        None,
        "front matter alone"
    );
    assert_eq!(
        preview("---\ntitle: Plan\n---"),
        None,
        "closed on the last line"
    );
    assert_eq!(
        preview("---\ntitle: Plan"),
        Some("title: Plan".into()),
        "unclosed, no line end"
    );
    assert_eq!(
        preview("---\n---\nBody."),
        Some("Body.".into()),
        "closed at once: two thematic breaks"
    );
    assert_eq!(
        preview("---\ntitle: Plan\n...\nBody."),
        Some("Body.".into()),
        "`...` closes"
    );
    assert_eq!(
        preview("---  \ntitle: Plan\n---  \nBody."),
        Some("Body.".into()),
        "trailing spaces"
    );
    assert_eq!(
        preview("---\n\ntitle: Plan\n---\nBody."),
        Some("title: Plan".into()),
        "a blank first line: no front matter"
    );
}

#[test]
fn a_setext_heading_naming_the_note_gives_way() {
    assert_eq!(preview("Plan\n====\nBody."), Some("Body.".into()));
    assert_eq!(preview("Plan\n---\n\nBody."), Some("Body.".into()));
    assert_eq!(preview("Other\n====\nBody."), Some("Other".into()));
    assert_eq!(
        preview("Plan\nmore\n===\nBody."),
        Some("Plan".into()),
        "only the next line is read"
    );
    assert_eq!(preview("Plan\n- a"), Some("Plan".into()), "a paragraph");
    assert_eq!(
        preview("Plan\n# Other"),
        Some("Plan".into()),
        "the next heading"
    );
    assert_eq!(preview("\n\nPlan\n=\nBody."), Some("Body.".into()));
    assert_eq!(
        preview("> # Plan\nBody."),
        Some("Body.".into()),
        "in a quote"
    );
    assert_eq!(
        preview("- # Plan\nBody."),
        Some("Body.".into()),
        "in an item"
    );
}

#[test]
fn blank_lines_are_skipped() {
    assert_eq!(
        preview("\n\n  \n\t\n# Plan\n\n\n  Body.\n"),
        Some("Body.".into())
    );
}

#[test]
fn block_markers_are_stripped() {
    for (text, shown) in [
        ("> quote", "quote"),
        ("- item", "item"),
        ("* item", "item"),
        ("1. item", "item"),
        ("1) item", "item"),
        ("- [ ] task", "task"),
        ("> - [x] done", "done"),
        ("   - indented", "indented"),
    ] {
        assert_eq!(preview(text), Some(shown.into()), "{text:?}");
    }
}

#[test]
fn emphasis_markers_are_stripped() {
    assert_eq!(
        preview("**bold** and _em_ and `code`"),
        Some("bold and em and code".into())
    );
    assert_eq!(preview("snake_case_name"), Some("snake_case_name".into()));
}

#[test]
fn a_line_blank_after_stripping_is_skipped() {
    assert_eq!(preview("\n---\n- \n>\n#\nBody."), Some("Body.".into()));
    assert_eq!(preview("# Plan\n***\n- \n"), None, "nothing left");
}

#[test]
fn a_huge_marked_line_gives_a_160_byte_preview() {
    let text = format!("# Plan\n> **é** {}", "é ".repeat(3_500_000));
    assert_eq!(
        preview(&text),
        Some(format!("é {}", "é ".repeat(52))),
        "160 bytes after stripping, cut on a char"
    );
}

#[test]
fn a_skipped_line_longer_than_the_parsed_bytes_is_skipped_whole() {
    let text = format!("- {}tail\nBody.", " ".repeat(2000));
    assert_eq!(preview(&text), Some("Body.".into()));
}

#[test]
fn a_skipped_line_past_the_scan_limit_is_none() {
    let text = format!("# Plan{}\nBody.", " ".repeat(2 << 20));
    assert_eq!(preview(&text), None);
    let text = format!("# Plan{}\nBody.", " ".repeat(1 << 19));
    assert_eq!(preview(&text), Some("Body.".into()));
}

#[test]
fn an_empty_query_shows_the_first_prose_line() {
    let text = "---\na: b\n---\n# Plan\n- **Body**";
    let (vault, index) = with_texts(&[("plan.md", Some(text))]);
    let recent = NoteQuery::new("", None, &[id(&vault, "plan.md")]);
    let found = found(&index, &recent);
    let cancel = AtomicBool::new(false);
    let preview = excerpt(&index.snapshot(), &found, &recent, text, &cancel);
    assert_eq!(preview.map(|preview| preview.text), Some("Body".into()));
}

#[test]
fn a_cancel_set_during_the_walk_gives_none() {
    let cancel = AtomicBool::new(false);
    // The stem is read once the heading is found, so the walk goes on cancelled.
    let stem = ["plan"]
        .into_iter()
        .inspect(|_| cancel.store(true, Ordering::Relaxed));
    assert_eq!(preview::preview("# Plan\nBody.", stem, &cancel), None);
    let stem = ["plan"].into_iter();
    assert_eq!(
        preview::preview("# Plan\nBody.", stem, &AtomicBool::new(false)).map(|found| found.text),
        Some("Body.".into())
    );
}

#[test]
fn a_cancel_set_during_the_front_matter_gives_none() {
    let text = "---\na: b\n---\nBody.";
    assert_eq!(
        preview::front_matter_end(text, &AtomicBool::new(false)),
        Some(13)
    );
    assert_eq!(
        preview::front_matter_end(text, &AtomicBool::new(true)),
        None
    );
    assert_eq!(
        preview::front_matter_end("Body.", &AtomicBool::new(true)),
        Some(0),
        "no front matter: nothing read"
    );
}

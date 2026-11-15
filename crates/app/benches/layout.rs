//! One keystroke's `shape_document` on a 500 KB note, real text system (incremental layout spec, D11).
//!
//!   cargo bench -p inkra-app --features bench --bench layout -- --quick

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use gpui::{BenchAppContext, Task};
use inkra_app::bench::Typing;

const CODE: &str = include_str!("../../highlight/benches/fixtures/rust-5000.txt");

const PROSE: &str = "## A section heading\n\nPlain prose with **strong**, *emphasis* and `code`, then a [link](https://example.com) and a [[Wiki Page|wikilink]].\nA second line of the same paragraph, long enough to wrap once at seventy columns or so.\n\n> A quote with *emphasis*.\n\n- a bullet with **bold**\n- [ ] a task\n\n";

struct Case {
    name: &'static str,
    text: String,
    caret: usize,
}

impl fmt::Display for Case {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name)
    }
}

impl Case {
    /// The caret sits inside the first `needle` past `fraction` of the text.
    fn new(name: &'static str, text: String, needle: &str, fraction: usize) -> Self {
        let from = text.ceil_char_boundary(text.len() * fraction / 100);
        let found = text[from..].find(needle).map_or(0, |at| from + at);
        Self {
            name,
            caret: found + needle.len() / 2,
            text,
        }
    }
}

/// Whole fixture lines from `*from`, at least `bytes` long, wrapping round.
fn code(from: &mut usize, bytes: usize) -> String {
    let mut out = String::new();
    while out.len() < bytes {
        if *from >= CODE.len() {
            *from = 0;
        }
        let end = CODE[*from..]
            .find('\n')
            .map_or(CODE.len(), |at| *from + at + 1);
        out.push_str(&CODE[*from..end]);
        *from = end;
    }
    out
}

/// A title, then 1000 × (a prose section, a Rust block): about 528 KB.
fn many_blocks() -> String {
    let mut text = String::from("# Title\n\n");
    let mut from = 0;
    for _ in 0..1000 {
        text.push_str(PROSE);
        text.push_str("```rust\n");
        text.push_str(&code(&mut from, 500 - PROSE.len() - 12));
        text.push_str("```\n\n");
    }
    text
}

/// About 200 KB of prose, then one 300 KB Rust block.
fn one_block() -> String {
    let mut text = String::from("# Title\n\n");
    while text.len() < 200_000 {
        text.push_str(PROSE);
    }
    text.push_str("```rust\n");
    text.push_str(&code(&mut 0, 300_000));
    text.push_str("```\n");
    text
}

fn cases() -> Vec<Case> {
    vec![
        Case::new("1000-blocks-prose", many_blocks(), "Plain prose", 50),
        Case::new("one-block-prose", one_block(), "Plain prose", 20),
        Case::new("one-block-code", one_block(), "fn ", 70),
    ]
}

#[gpui::bench(inputs = cases(), input_name = "case", group = "layout/keystroke", sample_size = 10)]
fn keystroke(case: &Case, cx: &mut BenchAppContext) {
    let mut window = cx.add_empty_window();
    let typing = window.update(|window, _| Typing::new(case.text.clone(), case.caret, window));
    let typing = Rc::new(RefCell::new(typing));
    cx.bench_batched_task(
        {
            let typing = Rc::clone(&typing);
            move |_| typing.borrow_mut().keystroke()
        },
        move |(), _| {
            window.update(|window, _| typing.borrow_mut().shape(window));
            Task::ready(())
        },
    );
}

gpui::bench_group!(benches, keystroke);
gpui::bench_main!(benches);

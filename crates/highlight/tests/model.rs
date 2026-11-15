//! D12's model test: after any run of edits and hearings, the layer's marks equal a fresh
//! colouring of the current text (code highlighting spec, deviation 4).

use std::ops::Range;

use inkra_editor::Layer;
use inkra_highlight::{Batch, TokenClass, batch, block_marks, dirty};
use inkra_markdown::parse;
use inkra_text::{Buffer, ByteOffset, EditMark};
use proptest::prelude::*;
use proptest::sample::Index;

const NOTE: &str = "Prose with `inline code` and more.

```rust
let s = \"hi\";
call(s);
```

> ```rust
> // a comment
> let x = 1;
> ```

- an item

  ```rust
  fn a() {

      /* note */ b();
  }
  ```

  ```Rust
  let y = 2;
  ```

```nope
plain text
```

> -
>     ```rust
>     let z = 3;
>     ```

- a
  - b

    c

d

    ```rust
    let
    ```

- ```
  z
  - b
foo

    ```rust
    let
    ```

- a

  b

    ```rust
    let
    ```
";

const FRAGMENTS: &[&str] = &[
    "`",
    "```",
    "```rust\n",
    "```\n",
    "\n",
    " ",
    "  ",
    "> ",
    "- ",
    "\n-\n",
    "<div>",
    "<!--",
    "-->",
    "rust",
    "nope",
    "fn ",
    "\"",
    "//",
    "/*",
    "é",
    "x",
];

#[derive(Debug, Clone)]
enum Op {
    Insert(Index, &'static str),
    /// A number of chars, clipped at the end.
    Delete(Index, usize),
    Replace(Index, usize, &'static str),
}

#[derive(Debug, Clone)]
enum Step {
    /// Each op its own transaction, all heard at once.
    Edit(Vec<Op>),
    Undo,
    Redo,
    Lost,
    /// Edits heard as `Opened(None)`.
    Unheard(Vec<Op>),
    /// A fresh layer seeded with the first `k % (blocks + 1)` blocks, heard as `Opened(Some)`.
    Reopen(usize),
}

struct Model {
    buffer: Buffer,
    layer: Layer<TokenClass>,
    heard: EditMark,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    let fragment = || prop::sample::select(FRAGMENTS);
    prop_oneof![
        (any::<Index>(), fragment()).prop_map(|(at, frag)| Op::Insert(at, frag)),
        (any::<Index>(), 0usize..=6).prop_map(|(at, len)| Op::Delete(at, len)),
        (any::<Index>(), 0usize..=6, fragment())
            .prop_map(|(at, len, frag)| Op::Replace(at, len, frag)),
    ]
}

fn step_strategy() -> impl Strategy<Value = Step> {
    let ops = || prop::collection::vec(op_strategy(), 1..=4);
    prop_oneof![
        4 => ops().prop_map(Step::Edit),
        1 => Just(Step::Undo),
        1 => Just(Step::Redo),
        1 => Just(Step::Lost),
        1 => ops().prop_map(Step::Unheard),
        1 => any::<usize>().prop_map(Step::Reopen),
    ]
}

/// Replaces `len` chars from the char boundary `at` picks with `frag`.
fn apply(buffer: &mut Buffer, op: &Op) {
    let (at, len, frag) = match op {
        Op::Insert(at, frag) => (at, 0, *frag),
        Op::Delete(at, len) => (at, *len, ""),
        Op::Replace(at, len, frag) => (at, *len, *frag),
    };
    let text = buffer.text();
    let boundaries: Vec<_> = (0..=text.len())
        .filter(|&index| text.is_char_boundary(index))
        .collect();
    let start = *at.get(&boundaries);
    let end = text[start..]
        .char_indices()
        .nth(len)
        .map_or(text.len(), |(index, _)| start + index);
    buffer.transact(|buffer| {
        buffer.replace(ByteOffset::new(start)..ByteOffset::new(end), frag);
    });
}

fn whole(buffer: &Buffer) -> Range<ByteOffset> {
    ByteOffset::new(0)..buffer.len()
}

/// The app's listener, in its order: catch up, read the chunks, then publish the batch.
fn hear(model: &mut Model, dirty: Option<Range<ByteOffset>>) -> Result<(), TestCaseError> {
    let Model {
        buffer,
        layer,
        heard,
    } = model;
    prop_assert_eq!(
        layer.catch_up(buffer),
        Ok(()),
        "a short run stays in the log"
    );
    let held: Vec<_> = layer.chunks().collect();
    let markdown = parse(buffer.text());
    let Batch { clear, lexed } = batch(buffer.text(), &markdown, &held, dirty);
    let at = buffer.edit_mark();
    let published = layer.publish(buffer, clear, lexed, at);
    prop_assert!(published.is_ok(), "{:?}", published);
    *heard = at;
    Ok(())
}

fn hear_edits(model: &mut Model) -> Result<(), TestCaseError> {
    let splices = model.buffer.edits_since(model.heard);
    let splices = splices.ok_or_else(|| TestCaseError::fail("a short run stays in the log"))?;
    let dirty = dirty(splices);
    hear(model, dirty)
}

fn step(model: &mut Model, step: Step) -> Result<(), TestCaseError> {
    match step {
        Step::Edit(ops) => {
            for op in &ops {
                apply(&mut model.buffer, op);
            }
            hear_edits(model)
        }
        Step::Undo => {
            model.buffer.undo();
            hear_edits(model)
        }
        Step::Redo => {
            model.buffer.redo();
            hear_edits(model)
        }
        Step::Lost => {
            model.layer.clear();
            let whole = whole(&model.buffer);
            hear(model, Some(whole))
        }
        Step::Unheard(ops) => {
            for op in &ops {
                apply(&mut model.buffer, op);
            }
            let whole = whole(&model.buffer);
            hear(model, Some(whole))
        }
        Step::Reopen(k) => {
            let Model { buffer, layer, .. } = model;
            *layer = Layer::new(buffer.edit_mark());
            let markdown = parse(buffer.text());
            let blocks = &markdown.code_blocks;
            let seed = blocks[..k % (blocks.len() + 1)]
                .iter()
                .map(|block| {
                    let marks = block_marks(buffer.text(), &markdown.spans, block);
                    (block.range.clone(), block.shape, marks)
                })
                .collect();
            let seeded = layer.publish(buffer, Vec::new(), seed, buffer.edit_mark());
            prop_assert!(seeded.is_ok(), "{:?}", seeded);
            hear(model, None)
        }
    }
}

proptest! {
    #[test]
    fn marks_equal_a_fresh_colouring_after_every_step(
        steps in prop::collection::vec(step_strategy(), 1..=12),
    ) {
        let buffer = Buffer::new(NOTE);
        let at = buffer.edit_mark();
        let mut model = Model { buffer, layer: Layer::new(at), heard: at };
        hear(&mut model, None)?;
        for next in steps {
            step(&mut model, next)?;
            let text = model.buffer.text();
            let markdown = parse(text);
            let fresh: Vec<_> = markdown
                .code_blocks
                .iter()
                .flat_map(|block| block_marks(text, &markdown.spans, block))
                .collect();
            let marks: Vec<_> = model
                .layer
                .marks_in(whole(&model.buffer))
                .map(|(range, class)| (range, *class))
                .collect();
            prop_assert_eq!(marks, fresh, "{:?}", text);
            let chunks: Vec<_> = model.layer.chunks().map(|(range, _)| range).collect();
            let blocks: Vec<_> = markdown
                .code_blocks
                .iter()
                .map(|block| block.range.clone())
                .collect();
            prop_assert_eq!(chunks, blocks, "{:?}", text);
        }
    }
}

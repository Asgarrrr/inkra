//! D12's model test: after any run of edits, hearings and late landings, once the backlog is
//! drained, the layer's marks equal a fresh colouring of the current text (code highlighting spec,
//! deviation 4). Every hearing runs with no dirty range, so exactness alone carries it (deviation
//! 10), and every block it lexes lands from a backlog (D10).

use std::ops::Range;

use inkra_editor::Layer;
use inkra_highlight::{
    Batch, BlockChunk, TokenClass, backlog, batch, block_marks, lex_backlog, open_chunks,
};
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

```rust
```

```rs

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
    /// A fresh layer seeded with the first `k % (blocks + 1)` blocks, heard as
    /// `Opened { fresh: true }`; the backlog in flight lands into nothing.
    Reopen(usize),
    /// The backlog in flight, if any, lands.
    Land,
    /// Backlogs land until none is pending.
    Drain,
}

/// A backlog lexed at the edit mark `at`, not landed yet: the buffer's task in flight (D10).
struct Flight {
    chunks: Vec<BlockChunk>,
    at: EditMark,
}

struct Model {
    buffer: Buffer,
    layer: Layer<TokenClass>,
    flight: Option<Flight>,
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
        1 => any::<usize>().prop_map(Step::Reopen),
        2 => Just(Step::Land),
        1 => Just(Step::Drain),
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

/// The app's listener, in its order: catch up, read the chunks, publish the batch, then copy a
/// backlog unless one is in flight. A landing replans the same way (D10, step 5).
fn hear(model: &mut Model) -> Result<(), TestCaseError> {
    let Model { buffer, layer, .. } = model;
    prop_assert_eq!(
        layer.catch_up(buffer),
        Ok(()),
        "a short run stays in the log"
    );
    let held: Vec<_> = layer.chunks().collect();
    let markdown = parse(buffer.text());
    let Batch {
        clear,
        lexed,
        pending,
    } = batch(buffer.text(), &markdown, &held, None);
    let published = layer.publish(buffer, clear, lexed, buffer.edit_mark());
    prop_assert!(published.is_ok(), "{:?}", published);
    if model.flight.is_none() && !pending.is_empty() {
        let chunks = lex_backlog(backlog(buffer.text(), &markdown, &pending));
        let at = buffer.edit_mark();
        model.flight = Some(Flight { chunks, at });
    }
    Ok(())
}

/// The blocks a replan would leave pending now.
fn pending(model: &Model) -> usize {
    let text = model.buffer.text();
    let held: Vec<_> = model.layer.chunks().collect();
    batch(text, &parse(text), &held, None).pending.len()
}

/// D10's landing: the chunks at their edit mark while the log holds the splices since, else an
/// empty publish now; then a replan.
fn land(model: &mut Model) -> Result<(), TestCaseError> {
    let Some(Flight { chunks, at }) = model.flight.take() else {
        return Ok(());
    };
    prop_assert!(!chunks.is_empty(), "a landing lands no block");
    let Model { buffer, layer, .. } = model;
    let landed = if buffer.edits_since(at).is_some() {
        layer.publish(buffer, Vec::new(), chunks, at)
    } else {
        layer.publish(buffer, Vec::new(), Vec::new(), buffer.edit_mark())
    };
    prop_assert!(landed.is_ok(), "{:?}", landed);
    hear(model)
}

/// Lands backlogs until none is in flight. A landing copied at the current edit mark lands its
/// blocks exact, so each one leaves fewer pending; only the first can be stale, which bounds the
/// drain to one landing per pending block after it.
fn drain(model: &mut Model) -> Result<(), TestCaseError> {
    while let Some(flight) = &model.flight {
        let current = flight.at == model.buffer.edit_mark();
        let before = pending(model);
        land(model)?;
        let after = pending(model);
        prop_assert!(
            !current || after < before,
            "{} pending, then {}",
            before,
            after
        );
    }
    prop_assert_eq!(pending(model), 0);
    Ok(())
}

/// After a drain: the marks are a fresh colouring and the chunks are the blocks' ranges.
fn coloured(model: &Model) -> Result<(), TestCaseError> {
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
    Ok(())
}

fn step(model: &mut Model, step: Step) -> Result<(), TestCaseError> {
    match step {
        Step::Edit(ops) => {
            for op in &ops {
                apply(&mut model.buffer, op);
            }
        }
        Step::Undo => {
            model.buffer.undo();
        }
        Step::Redo => {
            model.buffer.redo();
        }
        Step::Lost => model.layer.clear(),
        Step::Reopen(k) => {
            let Model {
                buffer,
                layer,
                flight,
            } = model;
            *layer = Layer::new(buffer.edit_mark());
            *flight = None;
            let count = k % (parse(buffer.text()).code_blocks.len() + 1);
            let mut calls = 0;
            let seed = open_chunks(buffer.text(), || {
                calls += 1;
                calls <= count
            });
            let seeded = layer.publish(buffer, Vec::new(), seed, buffer.edit_mark());
            prop_assert!(seeded.is_ok(), "{:?}", seeded);
        }
        Step::Land => return land(model),
        Step::Drain => {
            drain(model)?;
            return coloured(model);
        }
    }
    hear(model)
}

proptest! {
    #[test]
    fn marks_equal_a_fresh_colouring_once_the_backlog_drains(
        steps in prop::collection::vec(step_strategy(), 1..=12),
    ) {
        let buffer = Buffer::new(NOTE);
        let at = buffer.edit_mark();
        let mut model = Model { buffer, layer: Layer::new(at), flight: None };
        hear(&mut model)?;
        for next in steps {
            step(&mut model, next)?;
        }
        drain(&mut model)?;
        coloured(&model)?;
    }
}

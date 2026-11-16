use inkra_markdown::parse;

use super::*;
use crate::block_marks;

/// A rust block whose range is `len` bytes, fences included.
fn rust_block(len: usize) -> String {
    let fences = "```rust\nlet \n```".len();
    format!("```rust\nlet {}\n```\n", "x".repeat(len - fences))
}

/// How many of the `pending` blocks of `text` one backlog takes.
fn taken(text: &str, pending: &[usize]) -> usize {
    lex_backlog(backlog(text, &parse(text), pending)).len()
}

#[test]
fn a_backlog_stops_before_the_block_past_task_bytes() {
    let half = TASK_BYTES / 2;
    let text = [rust_block(half), rust_block(half), rust_block(64)].join("\n");

    assert_eq!(taken(&text, &[0, 1, 2]), 2);
}

#[test]
fn a_first_block_over_task_bytes_still_goes() {
    let text = [rust_block(TASK_BYTES + 1), rust_block(64)].join("\n");

    assert_eq!(taken(&text, &[0, 1]), 1);
}

/// Blocks in a quote, in a list item, with empty lines, with no content, and one no backlog takes.
const NOTE: &str = "> ```rust
> let a = 1;
>
> fn b() {}
> ```

- item

  ```rust
  let c;

  mut d
  ```

```rust
```

```nope
let e
```

```rs
/* f */ g();
```
";

#[test]
fn lex_backlog_gives_the_marks_block_marks_gives_on_the_pending_blocks() {
    let markdown = parse(NOTE);
    assert_eq!(markdown.code_blocks.len(), 5);
    let pending = [0, 1, 2, 4];

    let lexed = lex_backlog(backlog(NOTE, &markdown, &pending));

    let expected: Vec<_> = pending
        .iter()
        .map(|&index| {
            let block = &markdown.code_blocks[index];
            let marks = block_marks(NOTE, &markdown.spans, block);
            (block.range.clone(), block.shape, marks)
        })
        .collect();
    let coloured = expected.iter().filter(|(_, _, marks)| !marks.is_empty());
    assert_eq!(coloured.count(), 3);
    assert_eq!(lexed, expected);
}

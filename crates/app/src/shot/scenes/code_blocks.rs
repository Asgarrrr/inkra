use gpui::WindowHandle;

use crate::shell::AppShell;
use crate::shot::stage::Stage;

const BLOCKS: &str = r#"A paragraph before the blocks.

```rust
let first = "a deliberately long first line that wraps across rows so the second row shows its place " + "and carries on well past the right edge of the column";
let x = 1;
```

> A quote line before its block.
> ```sh
> echo "in a quote"
> ls -la
> ```

- A list item before its block.

  ```py
  print("in a list")
  total = 1 + 2
  ```

```
no language here
second line
```

    indented line one
    indented line two

Body text after the blocks."#;

pub(super) fn concealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(BLOCKS);
    stage.focus(window);
    stage.keys(window, "cmd-up");
    window
}

pub(super) fn revealed(stage: &mut Stage) -> WindowHandle<AppShell> {
    let window = stage.open_note(BLOCKS);
    stage.focus(window);
    stage.keys(window, "cmd-up down down down shift-down shift-down");
    window
}

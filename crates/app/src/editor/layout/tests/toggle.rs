//! A toggle restyles every row of its item: a continuation and a lazy line carry the strike too
//! (task boxes spec, D9).

use gpui::{AppContext, TestAppContext, px};
use inkra_text::{Buffer, ByteOffset, Diff};

use super::{Inputs, assert_same};
use crate::theme::builtin;

#[gpui::test]
fn a_toggle_restyles_the_items_continuation_and_lazy_lines(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        let text = "- [ ] a\n  b\nc\n\nbody";
        let end = ByteOffset::new(text.len());
        let buffer = cx.new(|_| Buffer::new(text));
        let inputs = Inputs {
            selection: end..end,
            marked: None,
            wrap_width: None,
            margin: px(76.),
            code_block_wrap: false,
            theme: builtin::dark(),
        };
        let before = inputs.layout(&buffer, None, window, cx);
        buffer.update(cx, |buffer, _| {
            let task = inkra_markdown::parse(text).gutters[0].task.clone().unwrap();
            assert!(buffer.apply_diff(&Diff::new(buffer.version(), vec![task.flip()])));
        });
        let carried = inputs.layout(&buffer, Some(&before), window, cx);
        assert_same(&carried, &inputs.layout(&buffer, None, window, cx)).unwrap();
        let struck = carried.lines.iter().map(|placed| {
            let runs = &placed.line.shape.runs;
            runs.iter().any(|run| run.strikethrough.is_some())
        });
        assert_eq!(struck.collect::<Vec<_>>(), [true, true, true, false, false]);
    });
}

//! The box slot where the box is wider than `• ` (task boxes spec, D3): headless, `• ` is 1.2 times
//! the body size, so at 12 px it is 14.4 px, under `TASK_BOX_SIZE`.

use std::rc::Rc;

use gpui::{TestAppContext, px};
use inkra_text::{Buffer, ByteOffset};

use super::super::{BlockScrolls, LayoutInputs, shape_document};
use crate::decorations::Drawn;
use crate::theme::{TASK_BOX_SIZE, builtin};

#[gpui::test]
fn a_box_wider_than_a_bullet_is_the_slot(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, _| {
        let text = "- [ ] a\n- b\n\nbody";
        let buffer = Buffer::new(text);
        let markdown = Rc::new(inkra_markdown::parse(text));
        let end = ByteOffset::new(text.len());
        let mut style = window.text_style();
        style.font_size = px(12.).into();
        let layout = shape_document(
            &LayoutInputs {
                buffer: &buffer,
                markdown: &markdown,
                selection: &(end..end),
                marked: None,
                style: &style,
                wrap_width: None,
                margin: px(76.),
                code_block_wrap: false,
                theme: builtin::dark(),
                scrolls: &BlockScrolls::default(),
                decorations: Drawn::default(),
            },
            None,
            window,
        );
        let bullet_text = layout.x_for_offset(ByteOffset::new(10));
        assert!(bullet_text < TASK_BOX_SIZE, "`• ` is {bullet_text:?}");
        let (square, _) = layout.lines[0].line.task_box().unwrap();
        assert_eq!(square.left(), px(0.));
        assert_eq!(layout.x_for_offset(ByteOffset::new(6)), square.right());
    });
}

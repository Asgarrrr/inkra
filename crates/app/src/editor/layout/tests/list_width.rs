//! A number that fits a slot keeps the standard slot (task boxes spec, D3): headless, a glyph is
//! 0.6 times the body size, so at 12 px `1.` and the gap fit one slot.

use std::rc::Rc;

use gpui::{TestAppContext, px};
use inkra_text::{Buffer, ByteOffset};

use super::super::{BlockScrolls, LayoutInputs, shape_document};
use crate::decorations::Drawn;
use crate::theme::builtin;

#[gpui::test]
fn a_number_that_fits_a_slot_starts_its_text_where_a_bullet_does(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|window, _| {
        let text = "1. a\n\n- b\n\nbody";
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
        let x = |offset| layout.x_for_offset(ByteOffset::new(offset));
        assert_eq!(x(3), x(8));
    });
}

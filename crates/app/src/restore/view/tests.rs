use gpui::px;
use inkra_editor::{Goal, Selection};
use inkra_text::ByteOffset;

use super::SavedView;
use crate::editor::{TopLine, ViewState};

#[test]
fn a_view_keeps_its_direction_and_drops_its_goal() {
    let mut selection = Selection::at(ByteOffset(9));
    selection.set_head(ByteOffset(3), Goal::Column(12.5));
    let view = ViewState {
        selection,
        top: TopLine::new(ByteOffset(2), px(6.5)),
    };

    let back = ViewState::from(SavedView::from(&view));

    assert_eq!(back.selection.tail(), ByteOffset(9));
    assert_eq!(back.selection.head(), ByteOffset(3));
    assert_eq!(back.selection.goal(), Goal::None);
    assert_eq!(back.top, view.top);
}

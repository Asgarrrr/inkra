use gpui::px;
use inkra_editor::{Goal, Selection};
use inkra_text::ByteOffset;

use super::SavedView;
use crate::editor::{TopLine, ViewState};

#[test]
fn a_view_keeps_its_direction_and_drops_its_goal() {
    let mut selection = Selection::at(ByteOffset::new(9));
    selection.set_head(ByteOffset::new(3), Goal::Column(12.5));
    let view = ViewState {
        selection,
        top: TopLine::new(ByteOffset::new(2), px(6.5)),
        scrolls: Vec::new(),
    };

    let back = ViewState::from(SavedView::from(&view));

    assert_eq!(back.selection.tail(), ByteOffset::new(9));
    assert_eq!(back.selection.head(), ByteOffset::new(3));
    assert_eq!(back.selection.goal(), Goal::None);
    assert_eq!(back.top, view.top);
}

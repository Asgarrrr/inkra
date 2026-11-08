use std::path::Path;

use gpui::{Bounds, TestAppContext, point, px, size};

use super::{WINDOW_SIZE, placement, vault_root};
use crate::restore::SavedWindow;

/// One vault, one session file: the key hashes these bytes, so compare bytes, not `Path`'s components.
#[test]
fn a_vault_root_drops_a_trailing_slash_and_dot_components() {
    for spelling in ["/Users/me/Notes/", "/Users/me/./Notes", "/Users/me/Notes"] {
        assert_eq!(
            vault_root(Path::new(spelling)).unwrap().as_os_str(),
            "/Users/me/Notes"
        );
    }
}

#[gpui::test]
fn a_saved_window_opens_on_its_display_and_one_gone_or_off_its_display_centres(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        let display = cx.primary_display().expect("the test display");
        let saved = SavedWindow {
            display: display.uuid().unwrap().to_string(),
            x: 120.,
            y: 80.,
            width: 1200.,
            height: 760.,
        };
        let frame = Bounds::new(point(px(120.), px(80.)), size(px(1200.), px(760.)));
        assert_eq!(placement(Some(&saved), cx), (Some(display.id()), frame));

        let gone = SavedWindow {
            display: "a display unplugged since".to_owned(),
            ..saved.clone()
        };
        let centred = (None, Bounds::centered(None, WINDOW_SIZE, cx));
        assert_eq!(placement(Some(&gone), cx), centred);
        let off = SavedWindow {
            x: 2000.,
            ..saved.clone()
        };
        assert_eq!(placement(Some(&off), cx), centred);
        assert_eq!(placement(None, cx), centred);
    });
}

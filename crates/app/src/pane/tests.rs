use inkra_layout::Side;

use super::{CORE_PANES, sides};
use crate::backlinks::BACKLINKS;
use crate::shell::FILES;

#[test]
fn core_panes_are_files_left_then_backlinks_right() {
    assert_eq!(
        sides(CORE_PANES.iter().copied()),
        [(FILES, Side::Left), (BACKLINKS, Side::Right)]
    );
    assert_eq!([FILES.name(), BACKLINKS.name()], ["files", "backlinks"]);
}

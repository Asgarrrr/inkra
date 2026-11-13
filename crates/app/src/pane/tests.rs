use inkra_layout::Side;

use super::{CORE_PANES, sides};
use crate::backlinks::{BACKLINKS, BACKLINKS_PANE};
use crate::shell::FILES;

#[test]
fn files_is_the_core_pane_left_and_backlinks_a_feature_pane_right() {
    assert_eq!(sides(CORE_PANES.iter().copied()), [(FILES, Side::Left)]);
    assert_eq!(sides([&BACKLINKS_PANE]), [(BACKLINKS, Side::Right)]);
    assert_eq!([FILES.name(), BACKLINKS.name()], ["files", "backlinks"]);
}

use std::cell::RefCell;
use std::rc::Rc;

use gpui::TestAppContext;
use inkra_text::ByteOffset;

use super::{listed, open, search, selected, with_texts};
use crate::test_support::entry_id;
use crate::vault_search::HitActivated;

#[gpui::test]
fn up_and_down_skip_headers_and_stop_at_the_ends(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan\n\nplan"), ("b.md", "plan")]);
    let mut opened = open(cx, &fs);
    search(&mut opened, "p l a n");
    assert_eq!(listed(&mut opened).len(), 5);
    assert_eq!(selected(&mut opened), Some(1));

    opened.visual.simulate_keystrokes("down");
    assert_eq!(selected(&mut opened), Some(2));
    opened.visual.simulate_keystrokes("down");
    assert_eq!(selected(&mut opened), Some(4), "over b's header");
    opened.visual.simulate_keystrokes("down");
    assert_eq!(selected(&mut opened), Some(4), "the last hit");

    opened.visual.simulate_keystrokes("up up");
    assert_eq!(selected(&mut opened), Some(1));
    opened.visual.simulate_keystrokes("up");
    assert_eq!(selected(&mut opened), Some(1), "never a's header");
}

#[gpui::test]
fn a_new_query_selects_its_first_hit(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan\nplan")]);
    let mut opened = open(cx, &fs);
    search(&mut opened, "p l a n");
    opened.visual.simulate_keystrokes("down");
    assert_eq!(selected(&mut opened), Some(2));

    search(&mut opened, "backspace");

    assert_eq!(listed(&mut opened), ["# a (2)", "1: plan", "2: plan"]);
    assert_eq!(
        selected(&mut opened),
        Some(1),
        "only a refresh keeps the hit"
    );
}

#[gpui::test]
fn enter_confirms_the_selected_hit(cx: &mut TestAppContext) {
    let fs = with_texts(&[("a.md", "plan\nthe plan")]);
    let mut opened = open(cx, &fs);
    let a = entry_id(&opened.notes, &mut opened.visual, "a");
    let activated = Rc::new(RefCell::new(Vec::new()));
    let _subscription = opened.visual.update({
        let activated = Rc::clone(&activated);
        let picker = opened.picker.clone();
        move |_, cx| {
            cx.subscribe(&picker, move |_, event: &HitActivated, _| {
                activated.borrow_mut().push(event.clone());
            })
        }
    });
    search(&mut opened, "p l a n");

    opened.visual.simulate_keystrokes("down enter");

    assert_eq!(
        *activated.borrow(),
        [HitActivated {
            note: a,
            range: ByteOffset(9)..ByteOffset(13),
        }]
    );
}

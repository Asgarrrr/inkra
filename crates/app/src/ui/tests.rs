use std::{cell::Cell, path::Path, rc::Rc};

use gpui::{
    AssetSource, Context, IntoElement, Modifiers, ParentElement, Render, Styled, TestAppContext,
    Window, div,
};

use super::{Assets, IconButton, IconName};

#[test]
fn every_icon_loads() {
    for name in IconName::ALL {
        let Some(bytes) = Assets.load(name.path()).expect("load") else {
            panic!("{name:?} has no file");
        };
        assert!(bytes.starts_with(b"<svg"), "{name:?} is not an svg");
    }
}

#[test]
fn no_icon_file_is_dangling() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
    for file in std::fs::read_dir(&dir).expect("the icons directory") {
        let file = file.expect("an entry").path();
        if file.extension().is_none_or(|extension| extension != "svg") {
            continue;
        }
        let name = file.file_name().expect("a file name").to_string_lossy();
        let path = format!("icons/{name}");
        assert!(
            IconName::ALL.iter().any(|icon| icon.path() == path),
            "{path} is no `IconName`"
        );
    }
}

#[test]
fn unknown_path_is_none() {
    assert!(Assets.load("icons/nope.svg").expect("load").is_none());
}

#[test]
fn every_icon_has_a_distinct_path() {
    for (ix, name) in IconName::ALL.iter().enumerate() {
        for other in &IconName::ALL[ix + 1..] {
            assert_ne!(name.path(), other.path(), "{name:?} and {other:?}");
        }
    }
}

struct Probe {
    disabled: bool,
    clicks: Rc<Cell<usize>>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let clicks = Rc::clone(&self.clicks);
        div().size_full().child(
            IconButton::new("probe", IconName::X, "Probe")
                .disabled(self.disabled)
                .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
        )
    }
}

/// How many clicks a probe button counts after one click on its centre.
fn clicks(disabled: bool, cx: &mut TestAppContext) -> usize {
    let clicks = Rc::new(Cell::new(0));
    let probe = Probe {
        disabled,
        clicks: Rc::clone(&clicks),
    };
    let (_, cx) = cx.add_window_view(|_, _| probe);
    cx.run_until_parked();
    let selector = if disabled { "probe-disabled" } else { "probe" };
    let bounds = cx.debug_bounds(selector).expect("drawn");
    cx.simulate_click(bounds.center(), Modifiers::none());
    clicks.get()
}

#[gpui::test]
fn a_disabled_icon_button_drops_its_click(cx: &mut TestAppContext) {
    assert_eq!(clicks(false, cx), 1);
    assert_eq!(clicks(true, cx), 0);
}

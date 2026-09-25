//! A picker over fixed strings, for tests. A row starting with `#` is a header: shown, never selected.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{AnyElement, Context, IntoElement, ParentElement, SharedString, Task, div};

use super::{Picker, PickerDelegate};

/// Each query answers after this, so a test can type twice before the first answer.
pub(crate) const DELAY: Duration = Duration::from_millis(10);

/// `KIND` makes two picker types from one delegate, so a test can open one over the other.
pub(crate) struct Letters<const KIND: u8 = 0> {
    all: Vec<&'static str>,
    rows: Vec<&'static str>,
    pub(crate) confirmed: Rc<RefCell<Vec<&'static str>>>,
    /// Every query whose result reached the picker, in order.
    pub(crate) applied: Rc<RefCell<Vec<String>>>,
}

impl<const KIND: u8> Letters<KIND> {
    pub(crate) fn new(all: &[&'static str]) -> Self {
        Self {
            all: all.to_vec(),
            rows: Vec::new(),
            confirmed: Rc::default(),
            applied: Rc::default(),
        }
    }
}

impl<const KIND: u8> PickerDelegate for Letters<KIND> {
    fn key_context(&self) -> &'static str {
        "Letters"
    }

    fn label(&self) -> &'static str {
        "Letters"
    }

    fn placeholder(&self) -> &'static str {
        "Type a letter…"
    }

    fn empty_text(&self) -> Option<&'static str> {
        Some("Nothing")
    }

    fn match_count(&self) -> usize {
        self.rows.len()
    }

    fn is_selectable(&self, ix: usize) -> bool {
        self.rows.get(ix).is_some_and(|row| !row.starts_with('#'))
    }

    fn update_matches(&mut self, query: &str, cx: &mut Context<Picker<Self>>) -> Task<()> {
        let rows: Vec<&'static str> = self
            .all
            .iter()
            .copied()
            .filter(|row| row.contains(query))
            .collect();
        let query = query.to_owned();
        let applied = Rc::clone(&self.applied);
        cx.spawn(async move |picker, cx| {
            cx.background_executor().timer(DELAY).await;
            let _ = picker.update(cx, |picker, cx| {
                applied.borrow_mut().push(query);
                picker.delegate.rows = rows;
                picker.matches_updated(0, cx);
            });
        })
    }

    fn confirm(&mut self, ix: usize, _: &mut Context<Picker<Self>>) {
        if let Some(row) = self.rows.get(ix) {
            self.confirmed.borrow_mut().push(row);
        }
    }

    fn render_row(&self, ix: usize, _: bool, _: &mut Context<Picker<Self>>) -> AnyElement {
        div()
            .child(self.rows.get(ix).copied().unwrap_or_default())
            .into_any_element()
    }

    fn row_label(&self, ix: usize) -> SharedString {
        self.rows.get(ix).copied().unwrap_or_default().into()
    }
}

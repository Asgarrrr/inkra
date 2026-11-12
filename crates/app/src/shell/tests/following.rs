use std::sync::Arc;

use gpui::{
    Modifiers, MouseButton, TestAppContext, VisualTestContext, WindowHandle, point, px, size,
};
use inkra_index::Resolution;
use inkra_vault::mem_fs::MemFs;
use inkra_vault::{EntryId, Fs};

use crate::editor::LinkFollowed;
use crate::images::png;
use crate::keymap::key_bindings;
use crate::open::Open;
use crate::shell::AppShell;
use crate::test_support::entry_id;

use super::splits::{centre, check_layout, close, last, shown_in};
use super::{
    activate, at, check, editor, image_shown, keys, open, open_vault, select, shell_focused,
    showing, tab_files, vault_names, view_of,
};

/// A vault whose `a` is showing, its caret at 0 (`open_buffer` leaves it at the end), with the
/// index built.
fn showing_a(
    cx: &mut TestAppContext,
    files: &[(&str, &str)],
) -> (WindowHandle<AppShell>, VisualTestContext) {
    cx.update(|cx| cx.bind_keys(key_bindings()));
    let fs = Arc::new(MemFs::new());
    for (name, text) in files {
        fs.insert_file(format!("/vault/{name}"), *text);
    }
    let (window, sidebar, notes) = open_vault(cx, fs as Arc<dyn Fs>);
    cx.run_until_parked();
    let a = entry_id(&notes, cx, "a");
    activate(&sidebar, cx, a);
    cx.run_until_parked();
    assert_eq!(showing(&window, cx), Some(a));
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("cmd-up");
    (window, visual)
}

fn shown_text(window: &WindowHandle<AppShell>, cx: &mut TestAppContext) -> String {
    editor(window, cx).read_with(cx, |editor, cx| editor.buffer().read(cx).text().to_owned())
}

#[gpui::test]
fn cmd_enter_on_a_wikilink_opens_its_note(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[[b]] next"), ("b.md", "beta")]);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(shown_text(&window, cx), "beta");
}

#[gpui::test]
fn a_relative_link_resolves_from_the_showing_note(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(
        cx,
        &[
            ("a.md", "[x](sub/c.md)"),
            ("c.md", "wrong"),
            ("sub/c.md", "gamma"),
        ],
    );

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(shown_text(&window, cx), "gamma");
}

#[gpui::test]
fn a_url_opens_outside_the_app(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[x](https://x.org/p)")]);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(cx.opened_url().as_deref(), Some("https://x.org/p"));
    assert_eq!(shown_text(&window, cx), "[x](https://x.org/p)");
}

fn notice(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext) -> Option<String> {
    window
        .update(cx, |shell, _, _| {
            shell.notice.as_ref().map(|notice| notice.text.to_string())
        })
        .unwrap()
}

#[gpui::test]
fn a_link_with_an_unlisted_scheme_opens_nothing(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[x](file:///tmp/a.txt)")]);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(cx.opened_url(), None);
    assert!(notice(&window, &mut visual).is_some_and(|text| text.contains("file:///tmp/a.txt")));
    assert_eq!(shown_text(&window, cx), "[x](file:///tmp/a.txt)");
}

#[gpui::test]
fn a_link_with_an_unknown_scheme_opens_nothing(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[x](zzz-app:anything)")]);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(cx.opened_url(), None);
    assert!(notice(&window, &mut visual).is_some_and(|text| text.contains("zzz-app:anything")));
    assert_eq!(shown_text(&window, cx), "[x](zzz-app:anything)");
}

#[gpui::test]
fn a_mail_address_opens_outside_the_app(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[x](mailto:a@b.org)")]);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(cx.opened_url().as_deref(), Some("mailto:a@b.org"));
    assert_eq!(notice(&window, &mut visual), None);
}

#[gpui::test]
fn a_scheme_is_matched_without_regard_to_case(cx: &mut TestAppContext) {
    let (_window, mut visual) = showing_a(cx, &[("a.md", "[x](HTTPS://x.org/p)")]);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(cx.opened_url().as_deref(), Some("HTTPS://x.org/p"));
}

#[gpui::test]
fn an_unresolved_link_changes_nothing(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[[missing]]")]);
    let before = showing(&window, cx);

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(showing(&window, cx), before);
    assert_eq!(cx.opened_url(), None);
    let notes = window
        .update(cx, |shell, _, _| shell.notes.clone())
        .unwrap();
    assert_eq!(vault_names(&notes, cx), ["a.md"]);
}

/// Rebinding the note showing would put its caret back at 0.
#[gpui::test]
fn a_link_to_the_note_showing_keeps_its_caret(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "x [[a]]")]);
    visual.simulate_keystrokes("end");
    let before = editor(&window, cx).read_with(cx, |editor, _| editor.cursor());

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(
        editor(&window, cx).read_with(cx, |editor, _| editor.cursor()),
        before
    );
}

/// Following a link to the note showing is the user's last word, as a click on it is.
#[gpui::test]
fn following_a_link_to_the_note_showing_during_a_slow_load_keeps_it(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "[[a]]");
    fs.insert_file("/vault/b.md", "beta");
    let (window, sidebar, notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);
    cx.run_until_parked();
    let (a, b) = (entry_id(&notes, cx, "a"), entry_id(&notes, cx, "b"));
    activate(&sidebar, cx, a);
    cx.run_until_parked();

    let gate = fs.block_load("/vault/b.md");
    activate(&sidebar, cx, b);
    cx.run_until_parked();
    let link = inkra_markdown::links("[[a]]").remove(0);
    window
        .update(cx, |shell, _, cx| shell.follow(&link, Open::Here, cx))
        .unwrap_or_else(|_| panic!("the window is open"));
    gate.release();
    cx.run_until_parked();

    assert_eq!(showing(&window, cx), Some(a));
}

#[gpui::test]
fn cmd_shift_enter_on_a_wikilink_opens_its_note_in_a_new_tab(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[[b]] next"), ("b.md", "beta")]);
    let a = showing(&window, cx);

    visual.simulate_keystrokes("cmd-shift-enter");
    visual.run_until_parked();

    let (tabs, active) = tab_files(&window, cx);
    assert_eq!((tabs.len(), tabs[0], active), (2, a, 1));
    assert_eq!(shown_text(&window, cx), "beta");
    check(&window, cx);
}

/// The link resolves from the note it is written in, not from the new tab.
#[gpui::test]
fn a_relative_link_in_a_new_tab_resolves_from_the_note_left(cx: &mut TestAppContext) {
    let (window, visual) = showing_a(
        cx,
        &[
            ("a.md", "[x](sub/c.md)"),
            ("c.md", "wrong"),
            ("sub/c.md", "gamma"),
        ],
    );
    let a = showing(&window, cx);
    let link = inkra_markdown::links("[x](sub/c.md)").remove(0);

    editor(&window, cx).update(cx, |_, cx| {
        cx.emit(LinkFollowed {
            link,
            open: Open::NewTab,
        });
    });
    visual.run_until_parked();

    let (tabs, active) = tab_files(&window, cx);
    assert_eq!((tabs.len(), tabs[0], active), (2, a, 1));
    assert_eq!(shown_text(&window, cx), "gamma");
    check(&window, cx);
}

/// Two groups, the right one active and showing `b`, the left one still `a`.
fn assert_split_to_b(
    window: &WindowHandle<AppShell>,
    cx: &mut VisualTestContext,
    a: Option<EntryId>,
) {
    let groups = centre(window, cx);
    assert_eq!(groups.len(), 2);
    assert_eq!(last(window, cx), groups[1]);
    assert_eq!(shown_text(window, cx), "beta");
    assert_eq!(shown_in(window, cx, 0), a);
    check_layout(window, cx);
}

#[gpui::test]
fn cmd_alt_click_on_a_link_opens_it_in_a_split(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[[b]] next"), ("b.md", "beta")]);
    let a = showing(&window, cx);
    visual.simulate_keystrokes("right right");
    let on_link = editor(&window, cx)
        .read_with(cx, |editor, _| editor.caret_point())
        .expect("drawn")
        + point(px(3.), px(0.));

    let modifiers = Modifiers {
        platform: true,
        alt: true,
        ..Modifiers::none()
    };
    visual.simulate_mouse_down(on_link, MouseButton::Left, modifiers);
    visual.simulate_mouse_up(on_link, MouseButton::Left, modifiers);
    visual.run_until_parked();

    assert_split_to_b(&window, &mut visual, a);
    close(&mut visual);
}

#[gpui::test]
fn cmd_alt_enter_follows_the_link_in_a_split(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[[b]] next"), ("b.md", "beta")]);
    let a = showing(&window, cx);

    visual.simulate_keystrokes("cmd-alt-enter");
    visual.run_until_parked();

    assert_split_to_b(&window, &mut visual, a);
    close(&mut visual);
}

#[gpui::test]
fn a_link_split_refused_by_the_minimum_opens_a_new_tab(cx: &mut TestAppContext) {
    let (window, mut visual) = showing_a(cx, &[("a.md", "[[b]] next"), ("b.md", "beta")]);
    let a = showing(&window, cx);
    visual.simulate_resize(size(px(600.), px(600.)));
    visual.run_until_parked();

    visual.simulate_keystrokes("cmd-alt-enter");
    visual.run_until_parked();

    assert_eq!(centre(&window, &mut visual).len(), 1);
    let (tabs, active) = tab_files(&window, cx);
    assert_eq!((tabs.len(), tabs[0], active), (2, a, 1));
    assert_eq!(shown_text(&window, cx), "beta");
    check(&window, cx);
}

/// `/notes/a.md` launched by path beside a `b.md`, its editor focused and its caret at 0.
fn detached(cx: &mut TestAppContext, text: &str) -> (WindowHandle<AppShell>, VisualTestContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/notes/a.md", text);
    fs.insert_file("/notes/b.md", "beta");
    super::detached(cx, &fs)
}

#[gpui::test]
fn a_url_in_a_detached_note_opens_outside_the_app(cx: &mut TestAppContext) {
    let (window, mut visual) = detached(cx, "[x](https://x.org/p)");

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(visual.opened_url().as_deref(), Some("https://x.org/p"));
    assert_eq!(showing(&window, &mut visual), Some(EntryId::DETACHED));
}

#[gpui::test]
fn a_wikilink_in_a_detached_note_changes_nothing(cx: &mut TestAppContext) {
    let (window, mut visual) = detached(cx, "[[b]]");

    visual.simulate_keystrokes("cmd-enter");
    visual.run_until_parked();

    assert_eq!(visual.opened_url(), None);
    assert_eq!(showing(&window, &mut visual), Some(EntryId::DETACHED));
    assert_eq!(tab_files(&window, &mut visual).0.len(), 1);
}

/// `a.md` holding `text` showing, beside `photo.png` and `report.pdf`; returns the window, its
/// context, `a`, `photo` and the PDF.
fn beside_files(
    cx: &mut TestAppContext,
    text: &str,
) -> (
    WindowHandle<AppShell>,
    VisualTestContext,
    EntryId,
    EntryId,
    EntryId,
) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", text);
    fs.insert_bytes("/vault/photo.png", png(40, 30));
    fs.insert_bytes("/vault/report.pdf", b"%PDF".to_vec());
    let (window, notes, mut visual) = shell_focused(cx, &fs);
    let a = entry_id(&notes, &mut visual, "a");
    let photo = entry_id(&notes, &mut visual, "photo.png");
    let pdf = entry_id(&notes, &mut visual, "report.pdf");
    open(&window, &mut visual, a);
    (window, visual, a, photo, pdf)
}

/// What `link`, written in the note showing, resolves to.
fn resolved(window: &WindowHandle<AppShell>, cx: &mut TestAppContext, link: &str) -> Resolution {
    let link = inkra_markdown::links(link).remove(0);
    window
        .update(cx, |shell, _, cx| {
            let index = shell.panes().links.read(cx).index();
            index.resolve_link(shell.showing(), &link)
        })
        .expect("the shell is open")
}

fn follow_here(window: &WindowHandle<AppShell>, cx: &mut VisualTestContext, link: &str) {
    let link = inkra_markdown::links(link).remove(0);
    editor(window, cx).update(cx, |_, cx| {
        cx.emit(LinkFollowed {
            link,
            open: Open::Here,
        });
    });
    cx.run_until_parked();
}

#[gpui::test]
fn following_an_embed_opens_the_image_and_back_returns_to_the_note_with_its_view(
    cx: &mut TestAppContext,
) {
    let (window, mut visual, a, photo, _) = beside_files(cx, "![[photo.png]] tail");
    select(&window, &mut visual, 1..3);

    follow_here(&window, &mut visual, "![[photo.png]]");
    assert_eq!(image_shown(&window, &mut visual), Some(photo));

    keys(&mut visual, "cmd-[");

    assert_eq!(showing(&window, &mut visual), Some(a));
    assert_eq!(view_of(&window, &mut visual).0, at(1)..at(3));
    check(&window, &mut visual);
}

#[gpui::test]
fn following_a_link_to_a_pdf_opens_nothing(cx: &mut TestAppContext) {
    let (window, mut visual, a, _, pdf) = beside_files(cx, "[[report.pdf]] ![[report.pdf]]");
    let before = tab_files(&window, &mut visual);
    assert_eq!(
        resolved(&window, &mut visual, "[[report.pdf]]"),
        Resolution::File(pdf)
    );

    follow_here(&window, &mut visual, "[[report.pdf]]");
    follow_here(&window, &mut visual, "![[report.pdf]]");

    assert_eq!(showing(&window, &mut visual), Some(a));
    assert_eq!(image_shown(&window, &mut visual), None);
    assert_eq!(tab_files(&window, &mut visual), before);
    check(&window, &mut visual);
}

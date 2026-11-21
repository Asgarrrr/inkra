use std::path::{Path, PathBuf};

use inkra_layout::{Side, ViewId};
use serde_json::json;

use super::{
    Rejected, SavedAxis, SavedFileTab, SavedGroup, SavedMember, SavedTab, SavedView, SavedWindow,
    Session, dock_views,
};
use crate::backlinks::{BACKLINKS, BACKLINKS_PANE};
use crate::pane::sides;
use crate::shell::{FILES, FILES_PANE};

/// Files and backlinks, the panes a default shell docks.
fn known() -> Vec<(ViewId, Side)> {
    sides([&FILES_PANE, &BACKLINKS_PANE])
}

fn note(note: &str) -> SavedTab {
    SavedTab::File(SavedFileTab {
        file: Some(note.to_owned()),
        view: None,
    })
}

fn view(name: &str) -> SavedTab {
    SavedTab::View {
        view: name.to_owned(),
    }
}

fn group(tabs: Vec<SavedTab>, active: usize) -> SavedGroup {
    SavedGroup { tabs, active }
}

fn session() -> Session {
    Session::new(
        PathBuf::from("/Users/me/Notes"),
        0,
        SavedMember::Group(group(
            vec![
                SavedTab::File(SavedFileTab {
                    file: Some("daily/2026-09-30.md".to_owned()),
                    view: Some(SavedView {
                        tail: 12,
                        head: 40,
                        top: 0,
                        below: 0.,
                    }),
                }),
                note("projects/inkra.md"),
                SavedTab::File(SavedFileTab {
                    file: None,
                    view: None,
                }),
            ],
            1,
        )),
        None,
        None,
        [true, true],
    )
}

fn decoded(text: &str) -> Session {
    Session::decode(text, Path::new("/Users/me/Notes")).expect("a session of this vault")
}

/// The centre's one group's tabs.
fn tabs(session: &Session) -> &[SavedTab] {
    let Some(SavedMember::Group(group)) = &session.center else {
        panic!("one centre group: {:?}", session.center);
    };
    &group.tabs
}

#[test]
fn a_session_round_trips_through_its_text() {
    let session = session();
    let text = session.encode().unwrap();
    assert_eq!(decoded(&text), session);
}

#[test]
fn a_tab_with_no_view_writes_no_view_key() {
    let text = session().encode().unwrap();
    assert_eq!(text.matches("\"view\"").count(), 1, "{text}");
}

#[test]
fn corrupt_text_is_rejected() {
    let text = session().encode().unwrap();
    let cut = &text[..text.len() / 2];
    assert_eq!(
        Session::decode(cut, Path::new("/Users/me/Notes")),
        Err(Rejected::Corrupt)
    );
}

#[test]
fn another_version_is_rejected() {
    let mut session = session();
    session.version += 1;
    let text = session.encode().unwrap();
    assert_eq!(
        Session::decode(&text, Path::new("/Users/me/Notes")),
        Err(Rejected::Version)
    );
}

#[test]
fn an_unknown_version_is_refused() {
    for version in [0, 1, 2, 4, 1000] {
        let text = format!(r#"{{ "version": {version}, "root": "/Users/me/Notes" }}"#);
        assert_eq!(
            Session::decode(&text, Path::new("/Users/me/Notes")),
            Err(Rejected::Version),
            "{version}"
        );
    }
    assert_eq!(
        Session::decode(
            r#"{ "root": "/Users/me/Notes" }"#,
            Path::new("/Users/me/Notes")
        ),
        Err(Rejected::Corrupt)
    );
}

#[test]
fn another_root_is_rejected() {
    let text = session().encode().unwrap();
    assert_eq!(
        Session::decode(&text, Path::new("/Users/me/Other")),
        Err(Rejected::Root)
    );
}

#[test]
fn v2_round_trips_a_nested_tree() {
    let mut session = session();
    session.active = 2;
    session.center = Some(SavedMember::Split {
        split: SavedAxis::Horizontal,
        flexes: vec![1.5, 0.5],
        children: vec![
            SavedMember::Group(group(vec![note("a.md")], 0)),
            SavedMember::Split {
                split: SavedAxis::Vertical,
                flexes: vec![1.0, 1.0],
                children: vec![
                    SavedMember::Group(group(vec![note("b.md"), note("c.md")], 1)),
                    SavedMember::Group(group(vec![note("d.md")], 0)),
                ],
            },
        ],
    });
    session.left = Some(SavedMember::Group(group(vec![view("files")], 0)));
    session.shown = [true, false];
    let text = session.encode().unwrap();

    assert_eq!(decoded(&text), session);
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["center"]["split"], json!("horizontal"));
    assert_eq!(value["center"]["children"][1]["split"], json!("vertical"));
    assert_eq!(value["left"]["tabs"], json!([{ "view": "files" }]));
    assert_eq!(value["shown"], json!([true, false]));
}

#[test]
fn a_malformed_center_keeps_the_docks() {
    let empty_split = r#"{ "split": "vertical", "flexes": [], "children": [] }"#;
    let nested_empty = r#"{ "split": "vertical", "flexes": [1.0, 1.0],
        "children": [ { "tabs": [], "active": 0 }, { "split": "horizontal", "flexes": [], "children": [] } ] }"#;
    for center in [
        "5",
        "null",
        r#"{ "split": "diagonal" }"#,
        empty_split,
        nested_empty,
    ] {
        let text = format!(
            r#"{{ "version": 3, "root": "/Users/me/Notes", "active": 0, "center": {center},
              "left": {{ "tabs": [{{ "view": "files" }}], "active": 0 }} }}"#
        );
        let session = decoded(&text);
        assert_eq!(session.center, None, "{center}");
        assert_eq!(
            session.left,
            Some(SavedMember::Group(group(vec![view("files")], 0))),
            "{center}"
        );
    }
}

#[test]
fn a_malformed_dock_keeps_the_centre() {
    let text = r#"{ "version": 3, "root": "/Users/me/Notes", "active": 0,
      "center": { "tabs": [{ "file": "a.md" }], "active": 0 },
      "left": { "tabs": "files" }, "right": 5 }"#;

    let session = decoded(text);

    assert_eq!((&session.left, &session.right), (&None, &None));
    assert_eq!(tabs(&session), [note("a.md")]);
    assert_eq!(session.shown, [true, true]);
}

#[test]
fn a_view_tab_and_a_file_tab_decode_apart() {
    let text = r#"{ "version": 3, "root": "/Users/me/Notes", "active": 0,
      "center": { "tabs": [
        { "view": "files" },
        { "file": "a.md", "view": { "tail": 1, "head": 2, "top": 0, "below": 0.0 } },
        { "file": null }
      ], "active": 0 } }"#;

    let session = decoded(text);

    assert_eq!(
        tabs(&session),
        [
            view("files"),
            SavedTab::File(SavedFileTab {
                file: Some("a.md".to_owned()),
                view: Some(SavedView {
                    tail: 1,
                    head: 2,
                    top: 0,
                    below: 0.
                }),
            }),
            SavedTab::File(SavedFileTab {
                file: None,
                view: None
            }),
        ]
    );
}

#[test]
fn a_file_tab_round_trips() {
    let tab = SavedTab::File(SavedFileTab {
        file: Some("projects/2026/photo.png".to_owned()),
        view: None,
    });
    let text = serde_json::to_string(&tab).unwrap();

    assert_eq!(text, r#"{"file":"projects/2026/photo.png"}"#);
    assert_eq!(serde_json::from_str::<SavedTab>(&text).unwrap(), tab);
}

#[test]
fn an_unknown_view_name_is_dropped() {
    let text = r#"{ "version": 3, "root": "/Users/me/Notes", "active": 0,
      "left": { "tabs": [{ "view": "graph" }, { "view": "files" }], "active": 0 },
      "right": { "tabs": [{ "view": "backlinks" }], "active": 0 } }"#;
    let session = decoded(text);
    let docks = dock_views([session.left.as_ref(), session.right.as_ref()], &known());
    assert_eq!(docks, dock_views([None, None], &known()));
    assert_eq!(docks[0][0].0, [FILES]);
}

#[test]
fn docks_round_trip_through_the_file() {
    let mut session = session();
    session.right = Some(SavedMember::Group(group(
        vec![view("backlinks"), view("files")],
        0,
    )));
    session.shown = [false, true];

    let decoded = decoded(&session.encode().unwrap());

    assert_eq!(decoded, session);
    assert_eq!(
        dock_views([decoded.left.as_ref(), decoded.right.as_ref()], &known()),
        [Vec::new(), vec![(vec![BACKLINKS, FILES], 0)]]
    );
}

/// A note tab in a dock drops; the active index follows the views kept.
#[test]
fn a_note_tab_in_a_dock_is_dropped() {
    let left = SavedMember::Group(group(
        vec![view("backlinks"), note("a.md"), view("files")],
        1,
    ));
    assert_eq!(
        dock_views([Some(&left), None], &known()),
        [vec![(vec![BACKLINKS, FILES], 0)], Vec::new()]
    );
}

#[test]
fn the_docks_field_is_written_as_names() {
    let mut session = session();
    session.left = Some(SavedMember::Group(group(vec![view("files")], 0)));
    session.right = Some(SavedMember::Group(group(vec![view("backlinks")], 0)));
    let value: serde_json::Value = serde_json::from_str(&session.encode().unwrap()).unwrap();
    assert_eq!(value["left"]["tabs"], json!([{ "view": "files" }]));
    assert_eq!(value["right"]["tabs"], json!([{ "view": "backlinks" }]));
}

#[test]
fn no_docks_writes_no_docks_key() {
    let text = session().encode().unwrap();
    assert!(!text.contains("\"left\""), "{text}");
    assert!(!text.contains("\"right\""), "{text}");
}

/// A field a later build adds to a view tab must not turn it into a file tab or fail the file.
#[test]
fn a_view_tab_with_an_unknown_field_decodes() {
    let tab: SavedTab = serde_json::from_str(r#"{"view":"files","pinned":true}"#).unwrap();
    assert_eq!(tab, view("files"));
}

/// A dock written as one group, before docks held trees, reads as a one-group tree.
#[test]
fn a_v2_single_group_dock_decodes() {
    let text = r#"{ "version": 3, "root": "/Users/me/Notes", "active": 0,
      "center": { "tabs": [{ "file": "a.md" }], "active": 0 },
      "left": { "tabs": [{ "view": "files" }, { "view": "backlinks" }], "active": 1 } }"#;

    let session = decoded(text);

    assert_eq!(
        session.left,
        Some(SavedMember::Group(group(
            vec![view("files"), view("backlinks")],
            1
        )))
    );
    assert_eq!(
        dock_views([session.left.as_ref(), session.right.as_ref()], &known()),
        [vec![(vec![FILES, BACKLINKS], 1)], Vec::new()]
    );
}

#[test]
fn the_window_round_trips_and_a_malformed_one_keeps_the_rest() {
    let mut saved = session();
    saved.window = Some(SavedWindow {
        display: "37D8832A-2D66-02CA-B9F7-8F30A301B230".to_owned(),
        x: 120.,
        y: 80.,
        width: 1200.,
        height: 760.,
    });
    assert_eq!(decoded(&saved.encode().unwrap()), saved);

    let text = r#"{ "version": 3, "root": "/Users/me/Notes", "active": 0,
      "center": { "tabs": [{ "file": "a.md" }], "active": 0 },
      "window": { "display": "37D8832A", "x": "left" } }"#;
    let session = decoded(text);
    assert_eq!(session.window, None);
    assert_eq!(tabs(&session), [note("a.md")]);
}

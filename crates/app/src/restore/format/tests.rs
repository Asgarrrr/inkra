use std::path::{Path, PathBuf};

use inkra_layout::View;
use serde_json::json;

use super::{
    Rejected, SavedAxis, SavedGroup, SavedMember, SavedNoteTab, SavedTab, SavedView, Session,
    dock_views, fnv1a, session_file,
};

fn note(note: &str) -> SavedTab {
    SavedTab::Note(SavedNoteTab {
        note: Some(note.to_owned()),
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
                SavedTab::Note(SavedNoteTab {
                    note: Some("daily/2026-09-30.md".to_owned()),
                    view: Some(SavedView {
                        tail: 12,
                        head: 40,
                        top: 0,
                        below: 0.,
                    }),
                }),
                note("projects/inkra.md"),
                SavedTab::Note(SavedNoteTab {
                    note: None,
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

/// The restore-tabs spec's example, verbatim: a version-1 file still reads.
#[test]
fn the_spec_example_decodes() {
    let text = r#"{
      "version": 1,
      "root": "/Users/me/Notes",
      "active": 1,
      "tabs": [
        { "note": "daily/2026-09-30.md", "view": { "tail": 12, "head": 40, "top": 0, "below": 0.0 } },
        { "note": "projects/inkra.md", "view": { "tail": 880, "head": 880, "top": 512, "below": 6.5 } },
        { "note": null }
      ]
    }"#;
    let session = decoded(text);
    let Some(SavedMember::Group(center)) = &session.center else {
        panic!("one centre group");
    };
    assert_eq!(center.active, 1);
    assert_eq!(
        center.tabs[1],
        SavedTab::Note(SavedNoteTab {
            note: Some("projects/inkra.md".to_owned()),
            view: Some(SavedView {
                tail: 880,
                head: 880,
                top: 512,
                below: 6.5
            }),
        })
    );
    assert_eq!(
        center.tabs[2],
        SavedTab::Note(SavedNoteTab {
            note: None,
            view: None
        })
    );
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
    for version in [0, 3, 1000] {
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

/// A version-1 file: one tab on `a.md`, then `docks` verbatim as the `docks` field's text.
fn with_docks(docks: &str) -> String {
    format!(
        r#"{{ "version": 1, "root": "/Users/me/Notes", "active": 0, "tabs": [{{ "note": "a.md" }}], "docks": {docks} }}"#
    )
}

#[test]
fn a_file_without_docks_decodes_with_none() {
    let text =
        r#"{ "version": 1, "root": "/Users/me/Notes", "active": 0, "tabs": [{ "note": "a.md" }] }"#;
    let session = decoded(text);
    assert_eq!((session.left, session.right), (None, None));
    assert_eq!(session.shown, [true, true]);
}

#[test]
fn a_malformed_docks_field_keeps_the_tabs() {
    for docks in ["5", "null", r#"{"left":{"views":[1]}}"#] {
        let session = decoded(&with_docks(docks));
        assert_eq!((&session.left, &session.right), (&None, &None), "{docks}");
        assert_eq!(tabs(&session), [note("a.md")], "{docks}");
    }
}

#[test]
fn a_v1_file_reads_as_one_centre_group_and_its_docks() {
    let text = r#"{
      "version": 1,
      "root": "/Users/me/Notes",
      "active": 1,
      "tabs": [ { "note": "a.md" }, { "note": "b.md" } ],
      "docks": {
        "left": { "views": [], "active": 0, "shown": false },
        "right": { "views": ["backlinks", "files"], "active": 1, "shown": true }
      }
    }"#;

    let session = decoded(text);

    assert_eq!(session.version, 2);
    assert_eq!(session.active, 0);
    assert_eq!(
        session.center,
        Some(SavedMember::Group(group(
            vec![note("a.md"), note("b.md")],
            1
        )))
    );
    assert_eq!(session.left, Some(SavedMember::Group(group(Vec::new(), 0))));
    assert_eq!(
        session.right,
        Some(SavedMember::Group(group(
            vec![view("backlinks"), view("files")],
            1
        )))
    );
    assert_eq!(session.shown, [false, true]);
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
            r#"{{ "version": 2, "root": "/Users/me/Notes", "active": 0, "center": {center},
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
    let text = r#"{ "version": 2, "root": "/Users/me/Notes", "active": 0,
      "center": { "tabs": [{ "note": "a.md" }], "active": 0 },
      "left": { "tabs": "files" }, "right": 5 }"#;

    let session = decoded(text);

    assert_eq!((&session.left, &session.right), (&None, &None));
    assert_eq!(tabs(&session), [note("a.md")]);
    assert_eq!(session.shown, [true, true]);
}

#[test]
fn a_view_tab_and_a_note_tab_decode_apart() {
    let text = r#"{ "version": 2, "root": "/Users/me/Notes", "active": 0,
      "center": { "tabs": [
        { "view": "files" },
        { "note": "a.md", "view": { "tail": 1, "head": 2, "top": 0, "below": 0.0 } },
        { "note": null }
      ], "active": 0 } }"#;

    let session = decoded(text);

    assert_eq!(
        tabs(&session),
        [
            view("files"),
            SavedTab::Note(SavedNoteTab {
                note: Some("a.md".to_owned()),
                view: Some(SavedView {
                    tail: 1,
                    head: 2,
                    top: 0,
                    below: 0.
                }),
            }),
            SavedTab::Note(SavedNoteTab {
                note: None,
                view: None
            }),
        ]
    );
}

#[test]
fn an_unknown_view_name_is_dropped() {
    let text = with_docks(
        r#"{
          "left": { "views": ["graph", "files"], "active": 0, "shown": true },
          "right": { "views": ["backlinks"], "active": 0, "shown": true }
        }"#,
    );
    let session = decoded(&text);
    let docks = dock_views([session.left.as_ref(), session.right.as_ref()]);
    assert_eq!(docks, dock_views([None, None]));
    assert_eq!(docks[0][0].0, [View::Files]);
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
        dock_views([decoded.left.as_ref(), decoded.right.as_ref()]),
        [Vec::new(), vec![(vec![View::Backlinks, View::Files], 0)]]
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
        dock_views([Some(&left), None]),
        [vec![(vec![View::Backlinks, View::Files], 0)], Vec::new()]
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

/// A field a later build adds to a view tab must not turn it into a note tab or fail the file.
#[test]
fn a_view_tab_with_an_unknown_field_decodes() {
    let tab: SavedTab = serde_json::from_str(r#"{"view":"files","pinned":true}"#).unwrap();
    assert_eq!(tab, view("files"));
}

/// A dock written as one group, before docks held trees, reads as a one-group tree.
#[test]
fn a_v2_single_group_dock_decodes() {
    let text = r#"{ "version": 2, "root": "/Users/me/Notes", "active": 0,
      "center": { "tabs": [{ "note": "a.md" }], "active": 0 },
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
        dock_views([session.left.as_ref(), session.right.as_ref()]),
        [vec![(vec![View::Files, View::Backlinks], 1)], Vec::new()]
    );
}

/// Published FNV-1a 64 vectors: the key must not change between builds.
#[test]
fn the_key_is_fnv_1a_64() {
    assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
}

/// This root hashes to a leading zero, which the name keeps: 16 digits always.
#[test]
fn the_session_file_is_named_by_the_root_key_in_application_support() {
    assert_eq!(
        session_file(Path::new("/Users/me"), Path::new("/Users/me/Notes1800")),
        Path::new("/Users/me/Library/Application Support/Inkra/sessions/088abcfd907476e2.json")
    );
}

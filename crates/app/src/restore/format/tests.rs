use std::path::{Path, PathBuf};

use inkra_layout::{Docks, Side, View};
use serde_json::json;

use super::{Rejected, SavedDocks, SavedTab, SavedView, Session, fnv1a, session_file};

fn session() -> Session {
    Session::new(
        PathBuf::from("/Users/me/Notes"),
        1,
        vec![
            SavedTab {
                note: Some("daily/2026-09-30.md".to_owned()),
                view: Some(SavedView {
                    tail: 12,
                    head: 40,
                    top: 0,
                    below: 0.,
                }),
            },
            SavedTab {
                note: Some("projects/inkra.md".to_owned()),
                view: None,
            },
            SavedTab {
                note: None,
                view: None,
            },
        ],
        None,
    )
}

#[test]
fn a_session_round_trips_through_its_text() {
    let session = session();
    let text = session.encode().unwrap();
    assert_eq!(
        Session::decode(&text, Path::new("/Users/me/Notes")),
        Ok(session)
    );
}

/// The spec's example, verbatim: the field names are the file's contract across builds.
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
    let session = Session::decode(text, Path::new("/Users/me/Notes")).unwrap();
    assert_eq!(session.active, 1);
    assert_eq!(
        session.tabs[1].view,
        Some(SavedView {
            tail: 880,
            head: 880,
            top: 512,
            below: 6.5
        })
    );
    assert_eq!(
        session.tabs[2],
        SavedTab {
            note: None,
            view: None
        }
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
fn another_root_is_rejected() {
    let text = session().encode().unwrap();
    assert_eq!(
        Session::decode(&text, Path::new("/Users/me/Other")),
        Err(Rejected::Root)
    );
}

/// One tab on `a.md`, then `docks` verbatim as the `docks` field's text.
fn with_docks(docks: &str) -> String {
    format!(
        r#"{{ "version": 1, "root": "/Users/me/Notes", "active": 0, "tabs": [{{ "note": "a.md" }}], "docks": {docks} }}"#
    )
}

fn decoded(text: &str) -> Session {
    Session::decode(text, Path::new("/Users/me/Notes")).expect("a session of this vault")
}

#[test]
fn a_file_without_docks_decodes_with_none() {
    let text =
        r#"{ "version": 1, "root": "/Users/me/Notes", "active": 0, "tabs": [{ "note": "a.md" }] }"#;
    assert_eq!(decoded(text).docks, None);
}

#[test]
fn a_malformed_docks_field_keeps_the_tabs() {
    for docks in ["5", "null", r#"{"left":{"views":[1]}}"#] {
        let session = decoded(&with_docks(docks));
        assert_eq!(session.docks, None, "{docks}");
        assert_eq!(session.tabs[0].note.as_deref(), Some("a.md"), "{docks}");
    }
}

#[test]
fn an_unknown_view_name_is_dropped() {
    let text = with_docks(
        r#"{
          "left": { "views": ["graph", "files"], "active": 0, "shown": true },
          "right": { "views": ["backlinks"], "active": 0, "shown": true }
        }"#,
    );
    let docks = decoded(&text).docks.expect("a docks field").docks();
    assert_eq!(docks, Docks::default());
    assert_eq!(docks.side_of(View::Files), Side::Left);
}

#[test]
fn docks_round_trip_through_the_file() {
    let mut docks = Docks::default();
    docks.move_view(View::Backlinks, Side::Left, 0);
    docks.toggle(Side::Right);
    let mut session = session();
    session.docks = Some(SavedDocks::from(&docks));

    let decoded = decoded(&session.encode().unwrap());

    assert_eq!(decoded, session);
    assert_eq!(decoded.docks.expect("a docks field").docks(), docks);
}

#[test]
fn the_docks_field_is_written_as_names() {
    let mut session = session();
    session.docks = Some(SavedDocks::from(&Docks::default()));
    let value: serde_json::Value = serde_json::from_str(&session.encode().unwrap()).unwrap();
    assert_eq!(value["docks"]["left"]["views"], json!(["files"]));
    assert_eq!(value["docks"]["right"]["views"], json!(["backlinks"]));
}

#[test]
fn no_docks_writes_no_docks_key() {
    let text = session().encode().unwrap();
    assert!(!text.contains("\"docks\""), "{text}");
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

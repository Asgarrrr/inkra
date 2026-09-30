use std::path::{Path, PathBuf};

use super::{Rejected, SavedTab, SavedView, Session, fnv1a, session_file};

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

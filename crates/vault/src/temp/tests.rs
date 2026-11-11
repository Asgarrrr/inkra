use super::{SUFFIX_UNITS, TEMP_EXTENSION, TEMP_MARK, is_temp, temp_path};
use std::path::{Path, PathBuf};

fn file_name(path: &Path) -> String {
    path.file_name()
        .expect("a name")
        .to_string_lossy()
        .into_owned()
}

fn units(name: &str) -> usize {
    name.encode_utf16().count()
}

#[test]
fn a_short_name_is_kept_whole() {
    let temp = temp_path(Path::new("/v/a.md"));
    let name = file_name(&temp);
    assert!(name.starts_with("a.md.inkra-"), "{name}");
    assert_eq!(temp.extension(), Some("tmp".as_ref()), "{name}");
}

#[test]
fn the_temp_of_the_longest_name_fits_and_stays_a_temp() {
    let note = PathBuf::from(format!("/v/{}.md", "a".repeat(252)));
    let name = file_name(&temp_path(&note));
    assert!(units(&name) <= 255, "{} units", units(&name));
    assert!(is_temp(name.as_ref()));
}

#[test]
fn the_temp_of_a_long_emoji_name_fits() {
    let note = PathBuf::from(format!("/v/{}.md", "😀".repeat(126)));
    let name = file_name(&temp_path(&note));
    assert!(units(&name) <= 255, "{} units", units(&name));
}

#[test]
fn the_room_is_the_suffix_at_its_longest() {
    let longest = format!("{TEMP_MARK}{}-{}{TEMP_EXTENSION}", u32::MAX, u64::MAX);
    assert_eq!(SUFFIX_UNITS, units(&longest));
}

#[test]
fn two_temps_of_one_note_differ() {
    let note = Path::new("/v/a.md");
    assert_ne!(temp_path(note), temp_path(note));
}

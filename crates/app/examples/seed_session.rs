//! Writes the session that opens `<note>` on the next launch of `<vault>`; `scripts/screenshot.sh` runs it.

use std::path::PathBuf;

fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let (Some(vault), Some(note), None) = (args.next(), args.next(), args.next()) else {
        return Err("usage: seed_session <vault> <note>".to_owned());
    };
    let note = note
        .into_string()
        .map_err(|_| "the note's path is not UTF-8".to_owned())?;
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    inkra_app::seed_session(&PathBuf::from(vault), &note, &PathBuf::from(home))
}

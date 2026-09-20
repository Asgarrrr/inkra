//! The command-line surface: what the binary does before a window exists.
//!
//! Spawns the real binary instead of calling `initial_note` directly. The thing
//! under test is argument handling, and the arguments belong to the process —
//! an in-process test would have to fake them and would then prove nothing
//! about `std::env::args_os`. Both cases exit before `open_window`, so this
//! stays headless; `check.sh` still renders nothing.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

/// The exit code and stderr of one run. `expect` is avoided so the crate's
/// `expect_used = "deny"` needs no exception here.
fn run(argument: &OsStr) -> (Option<i32>, String) {
    let output = match Command::new(env!("CARGO_BIN_EXE_inkra-app"))
        .arg(argument)
        .output()
    {
        Ok(output) => output,
        Err(error) => panic!("spawn inkra-app: {error}"),
    };
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A path under the temp directory that nothing creates, as raw bytes so the
/// non-UTF-8 case can splice an invalid byte into it.
fn absent_path(suffix: &[u8]) -> OsString {
    let mut bytes = std::env::temp_dir().into_os_string().into_vec();
    bytes.push(b'/');
    bytes.extend_from_slice(suffix);
    OsString::from_vec(bytes)
}

#[test]
fn a_missing_note_exits_one_and_names_the_path() {
    let path = absent_path(b"inkra-absent-note.md");
    let (code, stderr) = run(&path);

    assert_eq!(code, Some(1), "stderr: {stderr}");
    assert!(
        stderr.contains("inkra-absent-note.md"),
        "the message must name the path; stderr: {stderr}"
    );
}

/// `std::env::args` unwraps every argument into a `String`, so a filename that
/// is not valid UTF-8 panicked — and the panic crossed the macOS FFI boundary
/// as a non-unwinding abort, so the process died on SIGABRT with a backtrace
/// instead of naming the path.
/// A Unix path is bytes, and `RelPath::display_name` (`crates/vault/src/path.rs`)
/// already treats a non-UTF-8 name as renderable rather than fatal.
#[test]
fn a_non_utf8_argument_is_reported_not_aborted() {
    let path = absent_path(b"\xffinkra-absent-note.md");
    let (code, stderr) = run(&path);

    assert_eq!(code, Some(1), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}

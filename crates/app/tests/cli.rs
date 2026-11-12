use std::ffi::{OsStr, OsString};
use std::io::Read as _;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A window would keep the process alive: exiting on its own within the bound is the proof.
/// `expect` is avoided so the crate's `expect_used = "deny"` needs no exception here.
fn exits_within_bound(mut command: Command) -> (Option<i32>, String) {
    let mut child = match command.stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(error) => panic!("spawn inkra-app: {error}"),
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("inkra-app still ran after 10 s: it opened a window");
            }
            Err(error) => {
                let _ = child.kill();
                panic!("wait for inkra-app: {error}");
            }
        }
    };
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    (status.code(), stderr)
}

fn run(argument: &OsStr) -> (Option<i32>, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_inkra-app"));
    command.arg(argument);
    exits_within_bound(command)
}

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

/// The line is the logger's, so a `run` that installs no logger fails here.
#[test]
fn a_launch_failure_is_an_error_line_from_the_logger() {
    let path = absent_path(b"inkra-absent-vault");
    let (code, stderr) = run(&path);

    assert_ne!(code, Some(0), "stderr: {stderr}");
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("ERROR inkra_app") && line.contains("inkra-absent-vault")),
        "stderr: {stderr}"
    );
}

/// `std::env::args` unwraps every argument into a `String`; a non-UTF-8
/// filename would panic across the macOS FFI boundary as a non-unwinding
/// abort — SIGABRT instead of naming the path.
#[test]
fn a_non_utf8_argument_is_reported_not_aborted() {
    let path = absent_path(b"\xffinkra-absent-note.md");
    let (code, stderr) = run(&path);

    assert_eq!(code, Some(1), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}

/// Makes the folder listable again on drop, so it can be removed even when the test fails.
struct Unreadable(PathBuf);

impl Drop for Unreadable {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[test]
fn an_unreadable_vault_exits_one_before_any_window() {
    // Per process: two concurrent runs of this test must not share, and remove, one folder.
    let path = std::env::temp_dir().join(format!("inkra-unreadable-vault-{}", std::process::id()));
    // A run killed before its guard dropped leaves the folder unreadable.
    drop(Unreadable(path.clone()));
    if let Err(error) = std::fs::create_dir(&path) {
        panic!("create {}: {error}", path.display());
    }
    let dir = Unreadable(path);
    if let Err(error) = std::fs::set_permissions(&dir.0, std::fs::Permissions::from_mode(0o000)) {
        panic!("chmod 000: {error}");
    }

    let (code, stderr) = run(dir.0.as_os_str());

    assert_eq!(code, Some(1), "stderr: {stderr}");
    assert!(
        stderr.contains("inkra-unreadable-vault") && stderr.contains("ermission denied"),
        "the message must name the folder and the error; stderr: {stderr}"
    );
}

/// With `HOME` the folder itself, the session and cache places lie inside it: no vault can keep them.
#[test]
fn the_home_folder_as_the_argument_exits_one() {
    let dir = std::env::temp_dir().join(format!("inkra-home-vault-{}", std::process::id()));
    if let Err(error) = std::fs::create_dir_all(&dir) {
        panic!("create {}: {error}", dir.display());
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_inkra-app"));
    command.env("HOME", &dir).arg(&dir);
    let (code, stderr) = exits_within_bound(command);
    let _ = std::fs::remove_dir(&dir);

    assert_eq!(code, Some(1), "stderr: {stderr}");
    assert!(
        stderr.contains("holds Inkra's own files"),
        "stderr: {stderr}"
    );
}

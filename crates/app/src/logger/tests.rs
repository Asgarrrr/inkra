use log::{Level, Log as _, Metadata, Record};

use super::{LOGGER, line};

#[test]
fn a_line_is_the_level_the_target_and_the_message() {
    assert_eq!(
        line(
            &Record::builder()
                .level(Level::Warn)
                .target("inkra_app::vault")
                .args(format_args!("disk full"))
                .build()
        ),
        "WARN inkra_app::vault: disk full"
    );
}

fn enabled(level: Level, target: &str) -> bool {
    LOGGER.enabled(&Metadata::builder().level(level).target(target).build())
}

#[test]
fn inkra_targets_pass_from_warn() {
    assert!(enabled(Level::Error, "inkra_app::vault"));
    assert!(enabled(Level::Warn, "inkra_index"));
    assert!(!enabled(Level::Info, "inkra_app"));
}

#[test]
fn other_targets_pass_from_error_only() {
    assert!(enabled(Level::Error, "gpui_macos::text_system"));
    assert!(!enabled(Level::Warn, "gpui_macos::text_system"));
    assert!(!enabled(Level::Warn, "gpui"));
}

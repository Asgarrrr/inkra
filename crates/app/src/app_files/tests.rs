use std::path::Path;

use super::{cache_file, fnv1a, last_vault_file, session_file};

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
        session_file(Path::new("/Users/me"), Path::new("/Users/me/Notes1800")).as_deref(),
        Some(Path::new(
            "/Users/me/Library/Application Support/Inkra/sessions/088abcfd907476e2.json"
        ))
    );
}

/// Caches, not the vault: sync clients would copy the file and the watcher would see it (P4).
#[test]
fn the_cache_file_is_named_by_the_root_key_under_caches() {
    assert_eq!(
        cache_file(Path::new("/Users/me"), Path::new("/Users/me/Notes1800")).as_deref(),
        Some(Path::new(
            "/Users/me/Library/Caches/Inkra/index/088abcfd907476e2.idx"
        ))
    );
}

#[test]
fn a_vault_holding_the_caches_folder_gets_no_cache_file() {
    assert_eq!(
        cache_file(Path::new("/Users/me"), Path::new("/Users/me")),
        None
    );
}

#[test]
fn a_vault_holding_the_sessions_folder_gets_no_session_file() {
    assert_eq!(
        session_file(Path::new("/Users/me"), Path::new("/Users/me/Library")),
        None
    );
}

#[test]
fn the_last_vault_file_lives_in_application_support() {
    assert_eq!(
        last_vault_file(Path::new("/Users/me")),
        Path::new("/Users/me/Library/Application Support/Inkra/last-vault.json")
    );
}

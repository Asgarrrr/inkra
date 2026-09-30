use std::path::Path;

use super::vault_root;

/// One vault, one session file: the key hashes these bytes, so compare bytes, not `Path`'s components.
#[test]
fn a_vault_root_drops_a_trailing_slash_and_dot_components() {
    for spelling in ["/Users/me/Notes/", "/Users/me/./Notes", "/Users/me/Notes"] {
        assert_eq!(
            vault_root(Path::new(spelling)).unwrap().as_os_str(),
            "/Users/me/Notes"
        );
    }
}

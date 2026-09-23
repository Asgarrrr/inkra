use super::*;

#[test]
fn a_plain_relative_path_is_accepted() {
    let path = RelPath::new(Path::new("notes/today.md")).unwrap();
    assert_eq!(path.as_path(), Path::new("notes/today.md"));
}

#[test]
fn an_absolute_path_is_rejected() {
    assert!(RelPath::new(Path::new("/etc/passwd")).is_none());
}

#[test]
fn a_parent_component_is_rejected_anywhere_in_the_path() {
    assert!(RelPath::new(Path::new("../secrets.md")).is_none());
    assert!(RelPath::new(Path::new("notes/../../secrets.md")).is_none());
}

#[test]
fn a_current_dir_component_is_normalized_away() {
    let dotted = RelPath::new(Path::new("./a.md")).unwrap();
    assert_eq!(dotted, RelPath::new(Path::new("a.md")).unwrap());
    assert_eq!(dotted.depth(), 0);
    assert_eq!(
        RelPath::new(Path::new("notes/./today.md")).unwrap(),
        RelPath::new(Path::new("notes/today.md")).unwrap()
    );
}

#[test]
fn the_depth_is_the_component_count() {
    let path = RelPath::new(Path::new("a/b/c.md")).unwrap();
    assert_eq!(path.depth(), 2);
}

#[test]
fn starts_with_compares_components_not_bytes() {
    let dir = RelPath::new(Path::new("Projects")).unwrap();
    let sibling = RelPath::new(Path::new("Projects.md")).unwrap();
    let child = RelPath::new(Path::new("Projects/plan.md")).unwrap();
    assert!(!sibling.starts_with(&dir));
    assert!(child.starts_with(&dir));
}

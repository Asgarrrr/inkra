use std::path::Path;

use super::FileKind;

#[test]
fn an_image_extension_in_any_case_is_an_image() {
    let names = [
        "a.png",
        "a.jpg",
        "a.jpeg",
        "a.gif",
        "a.webp",
        "a.bmp",
        "a.tif",
        "a.tiff",
        "a.ico",
        "a.pbm",
        "a.pgm",
        "a.ppm",
        "a.jfif",
        "a.pnm",
        "a.pam",
        "PHOTO.PNG",
        "b.JPEG",
        "c.Png",
    ];
    for name in names {
        assert_eq!(
            FileKind::of(Path::new(name)),
            Some(FileKind::Image),
            "{name}"
        );
    }
}

#[test]
fn a_markdown_file_in_any_case_is_a_note() {
    for name in ["a.md", "B.MD"] {
        assert_eq!(
            FileKind::of(Path::new(name)),
            Some(FileKind::Note),
            "{name}"
        );
    }
}

#[test]
fn a_file_inkra_cannot_show_has_no_kind() {
    for name in ["a.svg", "a.pdf", "a.heic", "README"] {
        assert_eq!(FileKind::of(Path::new(name)), None, "{name}");
    }
}

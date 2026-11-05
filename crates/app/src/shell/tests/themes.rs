use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use gpui::TestAppContext;
use inkra_vault::Fs;
use inkra_vault::mem_fs::MemFs;

use super::open_vault;
use crate::theme::store::load;

#[gpui::test]
fn a_missing_theme_shows_a_notice(cx: &mut TestAppContext) {
    let fs = Arc::new(MemFs::new());
    fs.insert_file("/vault/a.md", "alpha");
    fs.insert_file("/vault/.inkra/settings.json", r#"{"theme":"Gone"}"#);
    let (window, _sidebar, _notes) = open_vault(cx, Arc::clone(&fs) as Arc<dyn Fs>);

    let resolved = block_on(load(&*fs, Path::new("/vault")));
    let notice = window
        .update(cx, |shell, _, cx| {
            shell.adopt_theme(resolved, cx);
            shell.notice.as_ref().map(|notice| notice.text.to_string())
        })
        .unwrap();
    assert_eq!(notice.as_deref(), Some(r#"theme "Gone" not found"#));
}

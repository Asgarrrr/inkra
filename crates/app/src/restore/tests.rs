use std::path::Path;
use std::sync::Arc;

use futures::executor::block_on;
use futures::lock::Mutex;
use inkra_vault::mem_fs::MemFs;

use super::replace_in_order;

/// The throttle's detached write can still run when the quit write lands: the older must not win.
#[test]
fn an_older_write_polled_last_does_not_overwrite_a_newer_one() {
    let fs = Arc::new(MemFs::new());
    let written = Arc::new(Mutex::new(0));
    let file = Path::new("/support/sessions/vault.json");
    let older = replace_in_order(
        Arc::clone(&written),
        fs.clone(),
        file.into(),
        "older".into(),
        1,
    );
    let newer = replace_in_order(
        Arc::clone(&written),
        fs.clone(),
        file.into(),
        "newer".into(),
        2,
    );

    block_on(newer);
    block_on(older);

    assert_eq!(fs.read_to_string(file).as_deref(), Some("newer"));
}

//! An operation anchored in a file no crate's module tree reaches, against a live server.
//!
//! rust-analyzer lists the symbols of such a file from its syntax tree and resolves nothing in it,
//! however long it is given: a hover there is `null` for ever. The anchor wait read that as a server
//! still loading and waited until its caller gave up — against the warm index, which has no caller
//! deadline, that was a `check --deep` that never answered. The server does say which case it is: a
//! pull diagnostic coded `unlinked-file`.
//!
//! Load-sensitive: one server at a time, enforced by the harness within this binary and by the
//! `rust-analyzer` test group in `.config/nextest.toml` across processes.

mod harness;

use harness::{
    a_rename_in, a_workspace_whose_module_file_no_module_declares, refusal_once_settled_from,
    AN_UNDECLARED_MODULE_FILE,
};

/// Against a settled server, as the warm index hands one over.
#[tokio::test(flavor = "multi_thread")]
async fn refuses_to_wait_on_a_file_no_module_declares() {
    // Given
    let workspace = a_workspace_whose_module_file_no_module_declares();
    let rename = a_rename_in(AN_UNDECLARED_MODULE_FILE, "face", "dial");

    // When
    let refusal = refusal_once_settled_from(&workspace, rename).await;

    // Then
    assert!(
        refusal.starts_with(&format!(
            "this seam cannot be cut here: {} is in no crate's module tree as rust-analyzer has \
             loaded it (\"This file is not included anywhere in the module tree",
            workspace.path().join(AN_UNDECLARED_MODULE_FILE).display()
        )),
        "the refusal does not name the undeclared file:\n{refusal}"
    );
}

//! `gh stack` membership is optional (`#workqueue` 9/14): a host whose `gh` lacks the extension reads as
//! "no membership", never as an error that would hide the rest of the lineage.

use std::os::unix::fs::PermissionsExt;

use tddy_github::gh_stack::gh_stack_membership;

#[tokio::test]
async fn without_the_gh_stack_extension_there_is_no_membership() {
    // Given — a `gh` that knows no `stack` command, as gh answers an unknown command
    let bin = tempfile::tempdir().unwrap();
    let gh = bin.path().join("gh");
    std::fs::write(
        &gh,
        "#!/bin/sh\necho \"unknown command \\\"$1\\\" for \\\"gh\\\"\" >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();

    // When
    let membership = gh_stack_membership(&gh, bin.path(), "feat/one").await;

    // Then
    assert_eq!(membership, None);
}

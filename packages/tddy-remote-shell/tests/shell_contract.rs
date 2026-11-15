//! The remote-shell contract, held by both shells this crate ships: the in-memory double and
//! `SshShell` driven through a stand-in `ssh` executable that runs commands on this host.

mod support;

use std::os::unix::fs::PermissionsExt as _;

use pretty_assertions::assert_eq;
use tddy_remote_shell::contract::{
    an_in_memory_shell_for_the_contract, assert_behaves_like_a_remote_shell,
};
use tddy_remote_shell::{FileMode, RemoteShell};

use support::an_ssh_that_runs_commands_here;

#[tokio::test]
async fn the_in_memory_shell_behaves_like_a_remote_shell() {
    // Given the in-memory shell, answering the contract's commands
    let shell = an_in_memory_shell_for_the_contract();

    // When / Then it is held to the contract
    assert_behaves_like_a_remote_shell(&shell, "/scratch").await;
}

#[tokio::test]
async fn the_ssh_shell_behaves_like_a_remote_shell() {
    // Given an ssh shell whose target runs commands
    let ssh = an_ssh_that_runs_commands_here();
    let shell = ssh.shell_on("buildbox");

    // When / Then it is held to the same contract
    assert_behaves_like_a_remote_shell(&shell, &ssh.scratch_dir().to_string_lossy()).await;
}

#[tokio::test]
async fn a_written_file_reads_back_byte_for_byte_with_its_mode() {
    // Given an ssh shell whose target runs commands, and bytes that are not text
    let ssh = an_ssh_that_runs_commands_here();
    let shell = ssh.shell_on("buildbox");
    let file = ssh.scratch_dir().join("work tree/.tddy/session.json");
    let path = file.to_string_lossy().into_owned();
    let bytes = vec![b'{', 0, 0xff, b'\r', b'\n', b'}'];

    // When they are written owner-only into a directory that does not exist yet, and read back
    shell
        .write_file(&path, &bytes, FileMode::OwnerOnly)
        .await
        .expect("the file is written");
    let read = shell.read_file(&path).await;

    // Then the port returns them, the target holds them, and only the owner can read them
    assert_eq!(read, Ok(bytes.clone()));
    assert_eq!(
        std::fs::read(&file).expect("the file is on the target"),
        bytes
    );
    assert_eq!(
        std::fs::metadata(&file)
            .expect("the file is on the target")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

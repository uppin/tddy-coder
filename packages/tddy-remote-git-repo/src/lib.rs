//! `tddy-remote-git-repo` — serve a tddy-daemon project as a git remote over LiveKit.
//!
//! Git execs this binary the way it execs `ssh`: `GIT_SSH_COMMAND=tddy-remote-git-repo` plus a
//! remote of the form `<daemon-instance-id>:<project>`. It resolves what git asked for
//! ([`ssh_argv`]), assembles credentials ([`credentials`]), asks the daemon over HTTP for a room
//! to join ([`daemon_rpc`]), and relays the local stdio of `git-upload-pack` /
//! `git-receive-pack` over `remote_git.RemoteGitService/Serve` ([`relay`]).
//!
//! Feature: docs/ft/daemon/remote-git-repo.md

pub mod credentials;
pub mod daemon_rpc;
pub mod relay;
pub mod ssh_argv;

pub use credentials::{
    resolve_credentials, CredentialArgs, CredentialError, Credentials, DaemonToken,
};
pub use daemon_rpc::{DaemonRpc, DaemonRpcError, RoomAdmission};
pub use ssh_argv::{parse_ssh_invocation, ArgvError, GitRequest, GitVerb};

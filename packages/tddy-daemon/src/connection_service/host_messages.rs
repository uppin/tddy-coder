use tddy_service::proto::connection::HostRemoteDesktop;
use tddy_service::proto::connection::ProbeOutcome as ProtoProbeOutcome;

use tddy_service::proto::connection::HostGithubCli;

use crate::ssh_agent_add::AgentAddFailure;

use tddy_service::proto::connection::AddHostKeyOutcome;

use tddy_service::proto::connection::AddHostKeyResponse;

use crate::ssh_agent_add::SshAgentKeyAdder;

use crate::host_keypair::HostKeypair;

use crate::host_prompts::AnswerRejection;

use tddy_service::proto::connection::SshAgentKey;

use tddy_service::proto::connection::HostSshAgent;

use tddy_service::proto::connection::HostGitIdentity;

use super::proto_worktree_size_status;

use tddy_service::proto::connection::WorktreeRow;

use crate::worktrees::WorktreeSizeStatus;

use crate::worktrees::WorktreeDiffRow;

/// Build a `WorktreeRow` from a worktree's branch/diff summary plus its current size state. The
/// size fields (`disk_bytes`, `size_status`, `size_calculated_at_unix_ms`) come from the
/// calculator; `disk_bytes`/timestamp are 0 until a size has been computed.
pub(crate) fn worktree_row_from_diff(
    diff: &WorktreeDiffRow,
    status: WorktreeSizeStatus,
    disk_bytes: Option<u64>,
    calculated_at_unix_ms: Option<i64>,
) -> WorktreeRow {
    WorktreeRow {
        path: diff.path.to_string_lossy().to_string(),
        branch_label: diff.branch_label.clone(),
        disk_bytes: disk_bytes.unwrap_or(0),
        changed_files: diff.changed_files,
        lines_added: diff.lines_added,
        lines_removed: diff.lines_removed,
        updated_at_unix_ms: calculated_at_unix_ms.unwrap_or(0),
        stale: false,
        size_status: proto_worktree_size_status(status) as i32,
        size_calculated_at_unix_ms: calculated_at_unix_ms.unwrap_or(0),
    }
}

/// Map a probe outcome to its wire enum, keeping "could not run" apart from any finding.
fn proto_probe_outcome(outcome: &crate::host_tooling::ProbeOutcome) -> ProtoProbeOutcome {
    match outcome {
        crate::host_tooling::ProbeOutcome::Ok => ProtoProbeOutcome::Ok,
        crate::host_tooling::ProbeOutcome::Failed(_) => ProtoProbeOutcome::Failed,
        crate::host_tooling::ProbeOutcome::Unsupported => ProtoProbeOutcome::Unsupported,
    }
}

/// The operator-facing reason a probe failed, empty for every other outcome.
fn probe_failure_reason(outcome: &crate::host_tooling::ProbeOutcome) -> String {
    match outcome {
        crate::host_tooling::ProbeOutcome::Failed(reason) => reason.clone(),
        _ => String::new(),
    }
}

/// Put a probed git identity on the wire.
///
/// `configured` carries whether an identity was found at all, so a host with none is distinguishable
/// from one whose probe failed — both would otherwise arrive as two empty strings, and an operator
/// reading a blank name cannot tell which of the two to go and fix.
pub(crate) fn git_identity_message(git: &crate::host_tooling::GitIdentity) -> HostGitIdentity {
    let (user_name, user_email) = git.name_and_email.clone().unwrap_or_default();
    HostGitIdentity {
        outcome: proto_probe_outcome(&git.outcome) as i32,
        configured: git.name_and_email.is_some(),
        user_name,
        user_email,
        failure_reason: probe_failure_reason(&git.outcome),
    }
}

/// Put a probed ssh-agent state on the wire.
///
/// `reachable` carries whether an agent answered at all, so "an agent holding nothing" and "no agent
/// at all" stay apart: both arrive with an empty key list, and they send an operator to two
/// different places — one to load a key, the other to start an agent.
///
/// No key carries a path. The agent knows a comment, which is free text, and does not know which
/// file an identity came from.
pub(crate) fn ssh_agent_message(agent: &crate::ssh_agent::AgentStatus) -> HostSshAgent {
    HostSshAgent {
        outcome: proto_probe_outcome(&agent.outcome) as i32,
        reachable: agent.reachable,
        keys: agent
            .keys
            .iter()
            .map(|key| SshAgentKey {
                key_type: key.key_type.clone(),
                fingerprint: key.fingerprint.clone(),
                comment: key.comment.clone(),
            })
            .collect(),
        failure_reason: probe_failure_reason(&agent.outcome),
    }
}

/// Why an answer was refused, in words for the operator who sent it.
///
/// The three cases read very differently to whoever is at the dialog: one says try again, one says
/// start over, and one says someone else already answered this.
///
/// "No prompt is waiting on that answer" also covers a prompt raised by a **different** operator,
/// deliberately: the two must be indistinguishable, or the endpoint tells any authenticated caller
/// which prompt ids are live. See [`HostPromptRegistry::answer`].
pub(crate) fn rejection_reason(rejection: &AnswerRejection) -> String {
    match rejection {
        AnswerRejection::UnknownPrompt => "no prompt is waiting on that answer".to_string(),
        AnswerRejection::Expired => "this prompt expired before the answer arrived".to_string(),
        AnswerRejection::AlreadyAnswered => "this prompt has already been answered".to_string(),
    }
}

/// The one thing an operator is told when their answer did not open the key.
///
/// A single constant used by **both** failing arms of [`unlock_and_add`] — the answer this host
/// could not decrypt and the passphrase that did not unlock the key — because the two must be
/// indistinguishable to the caller, and two separately written strings are two strings that drift.
const ANSWER_DID_NOT_UNLOCK: &str = "that passphrase did not unlock this key";

/// Decrypt an answer, unlock the key at `subject` with it, and hand the identity to `os_user`'s
/// agent — then drop the passphrase.
///
/// Blocking, and deliberately one function: the plaintext exists as a local of this call and of no
/// other, is never returned, never stored and never logged. The browser encrypting the answer is
/// undone by a single `debug!` here, so nothing on this path formats anything derived from it.
pub(crate) fn unlock_and_add(
    keypair: &dyn HostKeypair,
    adder: &dyn SshAgentKeyAdder,
    files: &dyn crate::host_private_key::HostUserFiles,
    os_user: &str,
    subject: &str,
    encrypted_answer: &[u8],
) -> AddHostKeyResponse {
    // As `os_user`, and only from inside `os_user`'s home: `subject` is free text from a browser,
    // and this daemon can reach files its caller cannot. See [`crate::host_private_key`].
    let locked = match crate::host_private_key::read_private_key(files, os_user, subject) {
        Ok(key) => key,
        Err(reason) => return add_key_failed(AddHostKeyOutcome::KeyUnreadable, reason),
    };
    let passphrase = match keypair.decrypt(encrypted_answer) {
        Ok(plaintext) => plaintext,
        // Answered with the **same** refusal as a passphrase that did not unlock the key, and
        // deliberately: a response that told the two apart would hand any authenticated session one
        // clean bit per chosen ciphertext against this host's long-lived RSA key — the input a
        // Manger-style attack on RSA-OAEP runs on, and `AddHostKey` → `AnswerHostPrompt` is a loop
        // anyone with a session can drive. `decrypt_blinded` closes the timing channel; only an
        // indistinguishable *answer* closes this one.
        //
        // What actually happened goes to the log instead, where the operator debugging their own
        // host can read it and a caller probing the endpoint cannot. The reason describes the
        // failure of the decrypt, never its input: no plaintext was recovered to leak.
        Err(reason) => {
            log::debug!(
                target: "tddy_daemon::connection_service",
                "AddHostKey: this host could not decrypt the answer to its own prompt, which is \
                 reported to the caller as a passphrase that did not unlock the key: {reason}"
            );
            return add_key_failed(
                AddHostKeyOutcome::WrongPassphrase,
                ANSWER_DID_NOT_UNLOCK.to_string(),
            );
        }
    };
    let unlocked = if locked.is_encrypted() {
        match locked.decrypt(&passphrase) {
            Ok(key) => key,
            // Nothing from the failure is carried out: `ssh-key` says only that the unlock did not
            // work, and the one thing an operator can do about it is type it again.
            Err(_) => {
                return add_key_failed(
                    AddHostKeyOutcome::WrongPassphrase,
                    ANSWER_DID_NOT_UNLOCK.to_string(),
                )
            }
        }
    } else {
        // A key that needs no passphrase, answered anyway. Reporting a wrong passphrase would be
        // untrue — `PrivateKey::decrypt` refuses an already-decrypted key rather than checking one.
        locked
    };
    // The plaintext has done its work and this is where it stops existing. Explicit rather than
    // left to the end of the function so the drop is visible at the point it is guaranteed.
    drop(passphrase);

    let fingerprint = unlocked.fingerprint(ssh_key::HashAlg::Sha256).to_string();
    match adder.add_identity(os_user, &unlocked) {
        Ok(()) => AddHostKeyResponse {
            added: true,
            outcome: AddHostKeyOutcome::Added as i32,
            fingerprint,
            failure_reason: String::new(),
        },
        Err(AgentAddFailure::Unreachable) => add_key_failed(
            AddHostKeyOutcome::NoAgent,
            format!("no ssh-agent is reachable for {os_user} on this host"),
        ),
        // An agent answered and said no, which is neither of the named failures: the key was read
        // and unlocked, and an agent is running. What it said is passed on for the operator to
        // report — it describes the exchange, never the key.
        Err(AgentAddFailure::Refused(reason)) => add_key_failed(
            AddHostKeyOutcome::Unspecified,
            format!("{os_user}'s ssh-agent refused the key: {reason}"),
        ),
    }
}

/// An add that put no key in the agent, saying which failure it was and why.
///
/// The reason is for an operator to read. It never quotes the answer, in any form — the response
/// message says so, and every caller here builds it from what the *host* did, never from what
/// arrived.
pub(crate) fn add_key_failed(outcome: AddHostKeyOutcome, reason: String) -> AddHostKeyResponse {
    AddHostKeyResponse {
        added: false,
        outcome: outcome as i32,
        fingerprint: String::new(),
        failure_reason: reason,
    }
}

/// Put a probed `gh` state on the wire. The login is the **host's**, not the calling session's.
pub(crate) fn github_cli_message(gh: &crate::host_tooling::GithubCliStatus) -> HostGithubCli {
    HostGithubCli {
        outcome: proto_probe_outcome(&gh.outcome) as i32,
        installed: gh.installed,
        authenticated: gh.authenticated,
        login: gh.login.clone().unwrap_or_default(),
        failure_reason: probe_failure_reason(&gh.outcome),
    }
}

/// Put one probed remote-desktop reading on the wire.
///
/// `can_bridge` and `desktop_reachable` are carried as two fields because they are two facts: "this
/// daemon has no bridge binary" and "nothing is serving a desktop here" have unrelated fixes, and a
/// single flag could not send an operator to the right one. `port` travels with them so an
/// unreachable reading is not read as authoritative for a host serving somewhere non-default.
pub(crate) fn host_remote_desktop_message(
    reading: &crate::remote_desktop_probe::DesktopReachability,
) -> HostRemoteDesktop {
    HostRemoteDesktop {
        outcome: proto_probe_outcome(&reading.outcome) as i32,
        // `screen_sharing.proto`'s `Protocol` values, which `DesktopProtocol`'s discriminants
        // mirror rather than restate — two enums meaning the same thing drift apart.
        protocol: reading.protocol as i32,
        can_bridge: reading.can_bridge,
        desktop_reachable: reading.desktop_reachable,
        port: u32::from(reading.port),
        failure_reason: probe_failure_reason(&reading.outcome),
    }
}

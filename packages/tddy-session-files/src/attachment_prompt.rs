//! The line a session's first prompt names its attached files with, by a path its agent can read.
//!
//! A start materializes its attachments into `<session dir>/artifacts/attachments/` before the
//! agent launches; nothing told the agent so, and on 2026-11-22 a child's first move was `find /`
//! (findings: `docs/dev/changesets/2026-11-22-coder-target-back-channel.md`, problem 5).
//! A local agent reads them where they are. An agent on a target cannot reach the daemon host's
//! session directory, so its start copies them into its worktree there
//! ([`TARGET_ATTACHMENTS_DIR`]) and names that path.

use std::path::Path;

use tddy_service::proto::session::SessionAttachment;
use tddy_workflow::session_attachments_root;

/// Where a target session's attachments are copied, relative to its worktree on the target: beside
/// the rest of the session's `.tddy` files, and git-excluded as they are.
pub const TARGET_ATTACHMENTS_DIR: &str = ".tddy/attachments";

/// Where an agent reads a session's attachments.
#[derive(Debug, Clone, Copy)]
pub enum AttachmentHome<'a> {
    /// On this host, in the session directory: each by its absolute path there.
    SessionDir(&'a Path),
    /// On a target, copied into the worktree: each by [`TARGET_ATTACHMENTS_DIR`]`/<basename>`,
    /// relative to the directory the agent starts in.
    TargetWorktree,
}

impl AttachmentHome<'_> {
    /// The path the agent reads the attachment `basename` at.
    pub fn path_of(&self, basename: &str) -> String {
        match self {
            Self::SessionDir(session_dir) => session_attachments_root(session_dir)
                .join(basename)
                .to_string_lossy()
                .into_owned(),
            Self::TargetWorktree => format!("{TARGET_ATTACHMENTS_DIR}/{basename}"),
        }
    }
}

/// The line naming every attachment that **materialized** — never one only asked for — with the
/// path the agent reads it at; `None` when there is none.
pub fn attachments_prompt_line(
    materialized: &[SessionAttachment],
    home: AttachmentHome<'_>,
) -> Option<String> {
    if materialized.is_empty() {
        return None;
    }
    let files: Vec<String> = materialized
        .iter()
        .map(|attachment| format!("- `{}`", home.path_of(&attachment.basename)))
        .collect();
    Some(format!(
        "Files were attached to this session for you to read:\n{}",
        files.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn attached(basename: &str) -> SessionAttachment {
        SessionAttachment {
            basename: basename.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_local_session_names_each_attachment_by_its_path_in_the_session_directory() {
        // Given two attachments materialized into a local session
        let session_dir = Path::new("/home/dev/.tddy/sessions/s-1");

        // When the line is written
        let line = attachments_prompt_line(
            &[attached("attachment-probe.md"), attached("diagram.png")],
            AttachmentHome::SessionDir(session_dir),
        );

        // Then each is named where the agent can open it
        assert_eq!(
            line.as_deref(),
            Some(
                "Files were attached to this session for you to read:\n\
                 - `/home/dev/.tddy/sessions/s-1/artifacts/attachments/attachment-probe.md`\n\
                 - `/home/dev/.tddy/sessions/s-1/artifacts/attachments/diagram.png`"
            )
        );
    }

    #[test]
    fn a_target_session_names_each_attachment_by_its_copy_in_the_worktree() {
        // When the line is written for a session on a target
        let line = attachments_prompt_line(
            &[attached("attachment-probe.md")],
            AttachmentHome::TargetWorktree,
        );

        // Then the attachment is named by its copy beside the session's other `.tddy` files
        assert_eq!(
            line.as_deref(),
            Some(
                "Files were attached to this session for you to read:\n\
                 - `.tddy/attachments/attachment-probe.md`"
            )
        );
    }

    #[test]
    fn a_session_with_no_attachment_names_none() {
        assert_eq!(
            attachments_prompt_line(&[], AttachmentHome::TargetWorktree),
            None
        );
    }
}

//! The login-time half of first-login enrolment, for a desktop deployment.
//!
//! A server daemon's `users:` is written by whoever installs it, and a login GitHub vouches for is
//! minted a session whether or not it is mapped — it is each token-gated RPC that refuses an
//! unmapped caller `permission_denied: user not mapped to OS user`. That refusal point is kept.
//!
//! A desktop install has nobody to write `users:`, so its first login is written down as it
//! completes: persisted into the config file the daemon was started from, applied to the shared
//! [`LiveUsers`] every service authorizes through, and only then minted. Every later RPC in the
//! same process resolves the operator at once. A later, different login meets the unchanged
//! lookup: it is minted a session exactly as on a server, and refused by the RPCs it calls.
//!
//! Which deployments enrol is decided where the daemon is assembled — an embedded desktop host
//! only. A server with an empty `users:` never has this admission, and refuses as it always has.
//!
//! Which *logins* enrol is decided by the transport the completing call arrived on, as the host
//! that received it stamped it: only [`RequestTransport::InProcess`], the desktop application's
//! own webview. The same `auth.AuthService` is served on the LiveKit common room and the agent
//! tool socket too, and a login completed over either is somebody who is not necessarily at this
//! machine — a room peer, a co-located process. On an unenrolled desktop such a login is not
//! enrolled and nothing is written; it completes as any unmapped login does, and its RPCs are
//! refused.

use std::path::PathBuf;

use tddy_daemon_kernel::first_login_enrolment::EnrolmentRefusal;
use tddy_daemon_kernel::live_users::LiveUsers;
use tddy_github::LoginAdmission;
use tddy_rpc::{RequestTransport, Status};

/// Enrols a desktop deployment's first login as the OS user the daemon runs as.
pub struct FirstLoginEnrolment {
    users: LiveUsers,
    config_path: PathBuf,
    os_user: String,
}

impl FirstLoginEnrolment {
    /// Enrol into `users`, persisting to `config_path` — the file the daemon was started from —
    /// and mapping the first login to `os_user`, the account this process runs as.
    pub fn new(users: LiveUsers, config_path: PathBuf, os_user: String) -> Self {
        Self {
            users,
            config_path,
            os_user,
        }
    }
}

impl LoginAdmission for FirstLoginEnrolment {
    fn admit(&self, github_login: &str, transport: RequestTransport) -> Result<(), Status> {
        if self.users.os_user_for_github(github_login).is_some() {
            return Ok(());
        }
        // Exhaustive on purpose: a transport added later must be decided here, not inherit
        // enrolment by falling into a wildcard.
        match transport {
            RequestTransport::InProcess => {}
            RequestTransport::LiveKit
            | RequestTransport::UnixSocket
            | RequestTransport::Pipe
            | RequestTransport::Http
            | RequestTransport::Grpc
            | RequestTransport::Direct => {
                log::warn!(
                    target: "tddy_daemon::auth",
                    "GitHub user {github_login} signed in over {transport:?}, not this desktop's \
                     own window; it is not enrolled, so its RPCs are refused"
                );
                return Ok(());
            }
        }
        match self
            .users
            .enrol_first_login(&self.config_path, github_login, &self.os_user)
        {
            Ok(mapping) => {
                log::info!(
                    target: "tddy_daemon::auth",
                    "enrolled GitHub user {} as this desktop's operator (OS user {}) in {}",
                    mapping.github_user,
                    mapping.os_user,
                    self.config_path.display()
                );
                Ok(())
            }
            // Signing in is not how a second account is added. The login completes as it would on
            // any daemon, and every RPC it makes is refused because the lookup does not map it.
            Err(EnrolmentRefusal::AlreadyEnrolled { github_user }) => {
                log::warn!(
                    target: "tddy_daemon::auth",
                    "GitHub user {github_login} signed in to a desktop enrolled to {github_user}; \
                     it is not mapped, so its RPCs are refused"
                );
                Ok(())
            }
            // Refused rather than admitted unrecorded: a login that appeared to work and was
            // unknown after the next restart is worse than one that says why it failed. The file
            // is named because on a desktop the person reading this is the one who can fix it.
            Err(refusal @ EnrolmentRefusal::ConfigNotWritable { .. }) => {
                log::error!(
                    target: "tddy_daemon::auth",
                    "could not enrol GitHub user {github_login}: {refusal}"
                );
                Err(Status::failed_precondition(format!(
                    "could not enrol GitHub user \"{github_login}\" as this desktop's operator: \
                     {refusal}"
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_daemon_kernel::config::{DaemonConfig, UserMapping};
    use tddy_rpc::Code;

    const THE_OPERATOR: &str = "operator";
    const THE_OS_USER: &str = "operator-os";

    /// An unenrolled desktop's config file, byte for byte: configured, and mapping nobody.
    const AN_UNENROLLED_CONFIG: &str = "repos_base_path: \"/tmp/repos\"\n";

    /// Every transport a login can arrive on that is not the desktop's own window.
    const NOT_THE_DESKTOPS_WINDOW: [RequestTransport; 6] = [
        RequestTransport::LiveKit,
        RequestTransport::UnixSocket,
        RequestTransport::Pipe,
        RequestTransport::Http,
        RequestTransport::Grpc,
        RequestTransport::Direct,
    ];

    /// A desktop deployment that has never enrolled anybody — in memory and in its config file.
    struct AnUnenrolledDesktop {
        users: LiveUsers,
        config_path: PathBuf,
        dir: tempfile::TempDir,
    }

    fn an_unenrolled_desktop() -> AnUnenrolledDesktop {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let config_path = dir.path().join("desktop.yaml");
        std::fs::write(&config_path, AN_UNENROLLED_CONFIG).expect("the config is written");
        AnUnenrolledDesktop {
            users: LiveUsers::default(),
            config_path,
            dir,
        }
    }

    impl AnUnenrolledDesktop {
        /// The admission this desktop's daemon is assembled with.
        fn admission(&self) -> FirstLoginEnrolment {
            FirstLoginEnrolment::new(
                self.users.clone(),
                self.config_path.clone(),
                THE_OS_USER.to_string(),
            )
        }

        /// Its config file as it now reads.
        fn config_file(&self) -> String {
            std::fs::read_to_string(&self.config_path).expect("the config file is readable")
        }

        /// Its config file, gone — the daemon can no longer record anything in it.
        fn whose_config_file_has_gone(self) -> (LiveUsers, PathBuf) {
            let Self {
                users,
                config_path,
                dir,
            } = self;
            drop(dir);
            (users, config_path)
        }
    }

    #[test]
    fn a_first_login_from_anywhere_but_the_desktops_own_window_is_admitted_unenrolled() {
        // Given one unenrolled desktop for each transport that is not its own window
        let desktops =
            NOT_THE_DESKTOPS_WINDOW.map(|transport| (transport, an_unenrolled_desktop()));

        // When the first login completes over each
        let outcomes: Vec<_> = desktops
            .iter()
            .map(|(transport, desktop)| {
                let admitted = desktop
                    .admission()
                    .admit(THE_OPERATOR, *transport)
                    .map_err(|status| status.code);
                (
                    *transport,
                    admitted,
                    desktop.users.snapshot(),
                    desktop.config_file(),
                )
            })
            .collect();

        // Then every one is admitted — to be minted unmapped, its RPCs refused — and nobody is
        // mapped or written down
        assert_eq!(
            outcomes,
            NOT_THE_DESKTOPS_WINDOW
                .map(|transport| (
                    transport,
                    Ok(()),
                    Vec::<UserMapping>::new(),
                    AN_UNENROLLED_CONFIG.to_string(),
                ))
                .to_vec()
        );
    }

    #[test]
    fn a_first_login_from_the_desktops_own_window_is_enrolled_and_persisted() {
        // Given an unenrolled desktop
        let desktop = an_unenrolled_desktop();

        // When its first login completes in its own window
        let admitted = desktop
            .admission()
            .admit(THE_OPERATOR, RequestTransport::InProcess)
            .map_err(|status| status.code);

        // Then the operator is mapped to the account the daemon runs as — at once, and in the file
        // the next start will read
        let reloaded =
            DaemonConfig::load(&desktop.config_path).expect("the rewritten config loads");
        assert_eq!(
            (
                admitted,
                desktop.users.snapshot(),
                reloaded.os_user_for_github(THE_OPERATOR),
            ),
            (
                Ok(()),
                vec![UserMapping {
                    github_user: THE_OPERATOR.to_string(),
                    os_user: THE_OS_USER.to_string(),
                }],
                Some(THE_OS_USER.to_string()),
            )
        );
    }

    #[test]
    fn a_first_login_whose_enrolment_cannot_be_recorded_is_refused_as_a_failed_precondition() {
        // Given an unenrolled desktop whose config file has gone
        let desktop = an_unenrolled_desktop();
        let admission = desktop.admission();
        let (users, _) = desktop.whose_config_file_has_gone();

        // When its first login completes in its own window
        let refusal = admission
            .admit(THE_OPERATOR, RequestTransport::InProcess)
            .expect_err("a login that cannot be recorded is not admitted");

        // Then it is refused naming why, and nobody is mapped
        const THE_REASON: &str = "could not enrol GitHub user \"operator\" as this desktop's \
                                  operator: the daemon config cannot be rewritten";
        assert_eq!(
            (
                refusal.code,
                refusal.message.get(..THE_REASON.len()),
                users.snapshot()
            ),
            (Code::FailedPrecondition, Some(THE_REASON), vec![]),
            "the refusal was {refusal:?}"
        );
    }
}

//! The workspaces of a signed-in person's Coder accounts, as hosts derived from this daemon.
//!
//! A second [`DerivedHostSource`], beside [`crate::derived_hosts::SshConfigDerivedHosts`]. It joins
//! three ports — who the person's Coder accounts are ([`CoderAccountSource`]), each account's
//! deployment URL and token ([`CoderCredentialSource`]), and the deployment's own listing
//! ([`CoderWorkspaceSource`]) — and never runs the `coder` CLI: whether the CLI is installed is
//! read from the host's tooling probe and only decides whether a workspace is *usable*.
//!
//! # Whose accounts
//!
//! [`DerivedHostSource::list`] is given an OS user, because an SSH config belongs to one. A Coder
//! account belongs to a *subject* — who the calling session's token resolved to — and two subjects
//! can share an OS user. So this source is built per call, for one subject
//! ([`CoderDerivedHosts::new`]), and ignores the OS user it is asked with: it lists that subject's
//! accounts and nobody else's.
//!
//! # Honesty
//!
//! - A person with no Coder account is `Ok` and empty.
//! - A vault that cannot be read — locked, or not askable — is an `Err` with the reason: the
//!   accounts themselves are unknown, so there is nothing to list and no workspace is invented.
//! - A workspace that is not running, or one on a daemon without the CLI, **is listed**, with
//!   `usable` false and the reason. Hiding it would read as an account that has no workspaces.
//! - One account that fails — a token its deployment rejects, a deployment nothing answers at —
//!   is listed as one unusable host standing for the account ([`coder_account_host_id`]), carrying
//!   the reason and no target. The other accounts' workspaces are listed as usual.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-15-remote-shell-coder-hosts.md.

use std::sync::Arc;

use async_trait::async_trait;

use crate::coder_workspaces::{
    CoderCredentialError, CoderCredentialSource, CoderWorkspace, CoderWorkspaceSource,
};
use crate::derived_hosts::{DerivedHost, DerivedHostKind, DerivedHostSource};
use crate::host_tooling::CoderCliStatus;

/// Why a running workspace cannot be used on a daemon that has no `coder` executable.
pub const CODER_CLI_NOT_INSTALLED: &str = "the coder CLI is not installed on this daemon";

/// One Coder account of a signed-in person: what a target names, and what an operator reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderAccountRef {
    /// The account's id, as the accounts listing names it and as a `CoderTarget` carries it.
    pub account_id: String,
    /// The label the person gave the account, or the one it was given when it was connected.
    pub label: String,
}

impl CoderAccountRef {
    pub fn new(account_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            label: label.into(),
        }
    }
}

/// Lists the Coder accounts a signed-in person has connected.
///
/// A port, because the vault is `tddy-accounts`' and this crate does not depend on it: the daemon
/// fills it from the vault records `ListAccounts` reads, filtered to the Coder provider.
pub trait CoderAccountSource: Send + Sync {
    /// `subject`'s Coder accounts, in the order the accounts listing shows them. `Ok` and empty is
    /// a person who connected none. A locked vault is
    /// [`CoderCredentialError::VaultLocked`]: which accounts there are is not known.
    fn accounts_of(&self, subject: &str) -> Result<Vec<CoderAccountRef>, CoderCredentialError>;
}

/// The accounts of a daemon that was started without Coder accounts: nobody has any. What
/// [`crate::HostServiceImpl`] lists from until the daemon wires its own.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCoderAccounts;

impl CoderAccountSource for NoCoderAccounts {
    fn accounts_of(&self, _subject: &str) -> Result<Vec<CoderAccountRef>, CoderCredentialError> {
        Ok(Vec::new())
    }
}

/// The id of `owner`'s workspace `workspace` of the account `account_id`, reached from the daemon
/// `parent_instance_id`: `<parent_instance_id>/coder/<account_id>/<owner>/<workspace>`.
///
/// The account is part of the id, so two accounts' workspaces of the same owner and name — two
/// deployments each with an `ada/devbox` — are two hosts.
#[must_use]
pub fn coder_derived_host_id(
    parent_instance_id: &str,
    account_id: &str,
    owner: &str,
    workspace: &str,
) -> String {
    format!("{parent_instance_id}/coder/{account_id}/{owner}/{workspace}")
}

/// The id of the host that stands for a whole account whose workspaces could not be listed:
/// `<parent_instance_id>/coder/<account_id>`.
#[must_use]
pub fn coder_account_host_id(parent_instance_id: &str, account_id: &str) -> String {
    format!("{parent_instance_id}/coder/{account_id}")
}

/// What an operator reads for a workspace: `<workspace> (<account label>)`.
#[must_use]
pub fn coder_host_label(workspace: &str, account_label: &str) -> String {
    format!("{workspace} ({account_label})")
}

/// Why a workspace whose deployment reports `status` cannot be used: `the workspace is <status>`.
/// The deployment's own word is kept, so `stopped`, `starting` and `failed` read differently.
#[must_use]
pub fn workspace_not_running_reason(status: &str) -> String {
    format!("the workspace is {status}")
}

/// The workspaces of one signed-in person's Coder accounts, as derived hosts of one daemon.
///
/// Each workspace is a host of kind [`DerivedHostKind::Coder`], identified by
/// [`coder_derived_host_id`], labelled by [`coder_host_label`] and targeted by
/// `CoderTarget { account_id, owner, workspace }` with no agent named. It is `usable` when its
/// deployment reports it running **and** this daemon has the `coder` CLI; otherwise
/// `unusable_reason` is [`workspace_not_running_reason`] for a workspace that is not running, and
/// [`CODER_CLI_NOT_INSTALLED`] for a running one the daemon cannot reach.
pub struct CoderDerivedHosts {
    parent_instance_id: String,
    subject: String,
    accounts: Arc<dyn CoderAccountSource>,
    credentials: Arc<dyn CoderCredentialSource>,
    workspaces: Arc<dyn CoderWorkspaceSource>,
    coder_cli: CoderCliStatus,
}

impl CoderDerivedHosts {
    /// A source listing `subject`'s accounts, whose hosts are derived from the daemon
    /// `parent_instance_id`, on a host whose `coder` CLI is `coder_cli`.
    #[must_use]
    pub fn new(
        parent_instance_id: impl Into<String>,
        subject: impl Into<String>,
        accounts: Arc<dyn CoderAccountSource>,
        credentials: Arc<dyn CoderCredentialSource>,
        workspaces: Arc<dyn CoderWorkspaceSource>,
        coder_cli: CoderCliStatus,
    ) -> Self {
        Self {
            parent_instance_id: parent_instance_id.into(),
            subject: subject.into(),
            accounts,
            credentials,
            workspaces,
            coder_cli,
        }
    }

    /// The one unusable host that stands for `account` when its workspaces cannot be listed:
    /// the account's label and the reason, and no target.
    fn account_host(&self, account: &CoderAccountRef, reason: String) -> DerivedHost {
        DerivedHost {
            id: coder_account_host_id(&self.parent_instance_id, &account.account_id),
            label: account.label.clone(),
            kind: DerivedHostKind::Coder as i32,
            parent_instance_id: self.parent_instance_id.clone(),
            target: None,
            usable: false,
            unusable_reason: reason,
        }
    }

    /// The derived host of one `workspace` of `account`: usable when the workspace is running
    /// and this daemon has the `coder` CLI, with the reason it is not otherwise.
    fn workspace_host(&self, account: &CoderAccountRef, workspace: &CoderWorkspace) -> DerivedHost {
        use tddy_service::proto::session::{session_target, CoderTarget, SessionTarget};

        let (usable, unusable_reason) = if !workspace.is_running() {
            (false, workspace_not_running_reason(&workspace.status))
        } else if !self.coder_cli.installed {
            (false, CODER_CLI_NOT_INSTALLED.to_string())
        } else {
            (true, String::new())
        };
        DerivedHost {
            id: coder_derived_host_id(
                &self.parent_instance_id,
                &account.account_id,
                &workspace.owner,
                &workspace.name,
            ),
            label: coder_host_label(&workspace.name, &account.label),
            kind: DerivedHostKind::Coder as i32,
            parent_instance_id: self.parent_instance_id.clone(),
            target: Some(SessionTarget {
                kind: Some(session_target::Kind::Coder(CoderTarget {
                    account_id: account.account_id.clone(),
                    owner: workspace.owner.clone(),
                    workspace: workspace.name.clone(),
                    agent: String::new(),
                })),
            }),
            usable,
            unusable_reason,
        }
    }

    /// Every workspace of `account` as a derived host, or the one host that stands for the
    /// account when its credentials or its deployment's listing fail.
    async fn hosts_of(&self, account: &CoderAccountRef) -> Vec<DerivedHost> {
        let credentials = match self
            .credentials
            .credentials_for(&self.subject, &account.account_id)
        {
            Ok(credentials) => credentials,
            Err(error) => return vec![self.account_host(account, error.to_string())],
        };
        let workspaces = match self.workspaces.workspaces(&credentials).await {
            Ok(workspaces) => workspaces,
            Err(error) => return vec![self.account_host(account, error.to_string())],
        };
        workspaces
            .iter()
            .map(|workspace| self.workspace_host(account, workspace))
            .collect()
    }
}

#[async_trait]
impl DerivedHostSource for CoderDerivedHosts {
    fn kind(&self) -> DerivedHostKind {
        DerivedHostKind::Coder
    }

    /// The workspaces of every Coder account of this source's subject: accounts in the accounts
    /// listing's order, each account's workspaces in its deployment's order. `os_user` is not
    /// read — see the module's *Whose accounts*.
    async fn list(&self, _os_user: &str) -> Result<Vec<DerivedHost>, String> {
        let accounts = self
            .accounts
            .accounts_of(&self.subject)
            .map_err(|error| match error {
                CoderCredentialError::VaultLocked => {
                    "the vault holding this Coder account is locked: unlock it and list again"
                        .to_string()
                }
                other => other.to_string(),
            })?;
        let mut hosts = Vec::new();
        for account in &accounts {
            hosts.extend(self.hosts_of(account).await);
        }
        Ok(hosts)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use tddy_credentials::SecretString;
    use tddy_service::proto::session::{session_target, CoderTarget, SessionTarget};

    use super::*;
    use crate::coder_workspaces::{CoderCredentials, CoderWorkspaceListError};
    use crate::derived_hosts::kind_segment;
    use crate::test_util::{
        a_coder_vault_of, a_coder_workspace, coder_deployments, ACoderVault, CoderDeployments,
    };

    const PARENT: &str = "daemon-a";
    /// Who is signed in: the owner of the vault.
    const ADA: &str = "ada-on-github";
    /// The OS user Ada's sessions run as on this daemon — shared with other people.
    const SHARED_OS_USER: &str = "dev";
    const WORK: &str = "coder-3f1c9a52@coder.example.com";
    const WORK_URL: &str = "https://coder.example.com";
    const LAB: &str = "coder-77aa01b3@coder.lab.example";
    const LAB_URL: &str = "https://coder.lab.example";

    fn credentials_at(url: &str) -> CoderCredentials {
        CoderCredentials::new(url, SecretString::new(format!("token-for-{url}")))
    }

    /// Ada's open vault holding her `Work` and `Lab` accounts.
    fn adas_vault() -> ACoderVault {
        a_coder_vault_of(ADA)
            .holding(CoderAccountRef::new(WORK, "Work"), credentials_at(WORK_URL))
            .holding(CoderAccountRef::new(LAB, "Lab"), credentials_at(LAB_URL))
    }

    fn the_cli_is_installed() -> CoderCliStatus {
        CoderCliStatus {
            installed: true,
            version: Some("v2.16.0".to_string()),
        }
    }

    /// What the source lists for `subject`, over `vault` and `deployments`, on a daemon whose CLI
    /// is `coder_cli`.
    async fn listed_for(
        subject: &str,
        vault: ACoderVault,
        deployments: CoderDeployments,
        coder_cli: CoderCliStatus,
    ) -> Result<Vec<DerivedHost>, String> {
        let vault = Arc::new(vault);
        CoderDerivedHosts::new(
            PARENT,
            subject,
            Arc::clone(&vault) as Arc<dyn CoderAccountSource>,
            vault as Arc<dyn CoderCredentialSource>,
            Arc::new(deployments),
            coder_cli,
        )
        .list(SHARED_OS_USER)
        .await
    }

    /// What Ada is listed, with the CLI installed.
    async fn listed_for_ada(
        vault: ACoderVault,
        deployments: CoderDeployments,
    ) -> Result<Vec<DerivedHost>, String> {
        listed_for(ADA, vault, deployments, the_cli_is_installed()).await
    }

    /// Ada's running workspace `workspace` of the account `account_id`, as it is listed.
    fn a_usable_host(account_id: &str, account_label: &str, workspace: &str) -> DerivedHost {
        DerivedHost {
            id: format!("{PARENT}/coder/{account_id}/ada/{workspace}"),
            label: format!("{workspace} ({account_label})"),
            kind: DerivedHostKind::Coder as i32,
            parent_instance_id: PARENT.to_string(),
            target: Some(SessionTarget {
                kind: Some(session_target::Kind::Coder(CoderTarget {
                    account_id: account_id.to_string(),
                    owner: "ada".to_string(),
                    workspace: workspace.to_string(),
                    agent: String::new(),
                })),
            }),
            usable: true,
            unusable_reason: String::new(),
        }
    }

    /// Whether each listed host can be used, and why not: its label, `usable`, `unusable_reason`.
    fn usability(
        listed: Result<Vec<DerivedHost>, String>,
    ) -> Result<Vec<(String, bool, String)>, String> {
        listed.map(|hosts| {
            hosts
                .into_iter()
                .map(|host| (host.label, host.usable, host.unusable_reason))
                .collect()
        })
    }

    #[test]
    fn a_workspaces_id_is_its_parent_then_coder_then_its_account_owner_and_name() {
        assert_eq!(
            coder_derived_host_id("daemon-a", WORK, "ada", "devbox"),
            format!("daemon-a/coder/{WORK}/ada/devbox")
        );
    }

    #[test]
    fn an_accounts_own_id_is_its_parent_then_coder_then_the_account() {
        assert_eq!(
            coder_account_host_id("daemon-a", WORK),
            format!("daemon-a/coder/{WORK}")
        );
    }

    #[test]
    fn a_workspace_is_labelled_with_its_name_and_its_accounts_label() {
        assert_eq!(coder_host_label("devbox", "Work"), "devbox (Work)");
    }

    #[test]
    fn a_workspace_that_is_not_running_says_what_it_is_instead() {
        assert_eq!(
            (
                workspace_not_running_reason("stopped"),
                workspace_not_running_reason("starting"),
            ),
            (
                "the workspace is stopped".to_string(),
                "the workspace is starting".to_string(),
            )
        );
    }

    #[test]
    fn the_coder_kind_is_the_coder_segment() {
        assert_eq!(kind_segment(DerivedHostKind::Coder), Some("coder"));
    }

    #[tokio::test]
    async fn the_coder_source_is_of_the_coder_kind() {
        // Given
        let source = CoderDerivedHosts::new(
            PARENT,
            ADA,
            Arc::new(NoCoderAccounts),
            Arc::new(adas_vault()),
            Arc::new(coder_deployments()),
            the_cli_is_installed(),
        );

        // When
        let kind = source.kind();

        // Then
        assert_eq!(kind, DerivedHostKind::Coder);
    }

    #[tokio::test]
    async fn lists_each_accounts_workspaces_in_the_accounts_order_targeting_each() {
        // Given two accounts, the first with two running workspaces and the second with one
        let deployments = coder_deployments()
            .listing(
                WORK_URL,
                vec![
                    a_coder_workspace("ada", "devbox", "running"),
                    a_coder_workspace("ada", "docs", "running"),
                ],
            )
            .listing(LAB_URL, vec![a_coder_workspace("ada", "gpu", "running")]);

        // When
        let listed = listed_for_ada(adas_vault(), deployments).await;

        // Then
        assert_eq!(
            listed,
            Ok(vec![
                a_usable_host(WORK, "Work", "devbox"),
                a_usable_host(WORK, "Work", "docs"),
                a_usable_host(LAB, "Lab", "gpu"),
            ])
        );
    }

    #[tokio::test]
    async fn a_person_with_no_coder_account_reaches_no_workspaces() {
        // Given an open vault holding no Coder account
        let vault = a_coder_vault_of(ADA);

        // When
        let listed = listed_for_ada(vault, coder_deployments()).await;

        // Then
        assert_eq!(listed, Ok(Vec::new()));
    }

    #[tokio::test]
    async fn the_accounts_listed_are_the_subjects_not_the_os_users() {
        // Given Ada's accounts, on a daemon where another person shares her OS user
        let deployments = coder_deployments().listing(
            WORK_URL,
            vec![a_coder_workspace("ada", "devbox", "running")],
        );

        // When that other person lists
        let listed = listed_for(
            "grace-on-github",
            adas_vault(),
            deployments,
            the_cli_is_installed(),
        )
        .await;

        // Then none of Ada's workspaces are theirs
        assert_eq!(listed, Ok(Vec::new()));
    }

    #[tokio::test]
    async fn a_workspace_that_is_not_running_is_listed_unusable_with_its_status() {
        // Given one running and one stopped workspace
        let deployments = coder_deployments()
            .listing(
                WORK_URL,
                vec![
                    a_coder_workspace("ada", "devbox", "running"),
                    a_coder_workspace("ada", "scratch", "stopped"),
                ],
            )
            .listing(LAB_URL, Vec::new());

        // When
        let listed = usability(listed_for_ada(adas_vault(), deployments).await);

        // Then both are listed, the stopped one saying what it is
        assert_eq!(
            listed,
            Ok(vec![
                ("devbox (Work)".to_string(), true, String::new()),
                (
                    "scratch (Work)".to_string(),
                    false,
                    "the workspace is stopped".to_string()
                ),
            ])
        );
    }

    #[tokio::test]
    async fn without_the_cli_a_running_workspace_is_unusable_and_a_stopped_one_still_says_stopped()
    {
        // Given one running and one stopped workspace, on a daemon with no coder executable
        let deployments = coder_deployments()
            .listing(
                WORK_URL,
                vec![
                    a_coder_workspace("ada", "devbox", "running"),
                    a_coder_workspace("ada", "scratch", "stopped"),
                ],
            )
            .listing(LAB_URL, Vec::new());

        // When
        let listed = usability(
            listed_for(
                ADA,
                adas_vault(),
                deployments,
                CoderCliStatus::not_installed(),
            )
            .await,
        );

        // Then
        assert_eq!(
            listed,
            Ok(vec![
                (
                    "devbox (Work)".to_string(),
                    false,
                    CODER_CLI_NOT_INSTALLED.to_string()
                ),
                (
                    "scratch (Work)".to_string(),
                    false,
                    "the workspace is stopped".to_string()
                ),
            ])
        );
    }

    #[tokio::test]
    async fn a_locked_vault_is_an_error_saying_to_unlock_it() {
        // Given
        let vault = adas_vault().locked();

        // When
        let listed = listed_for_ada(vault, coder_deployments()).await;

        // Then
        assert_eq!(
            listed,
            Err(
                "the vault holding this Coder account is locked: unlock it and list again"
                    .to_string()
            )
        );
    }

    #[tokio::test]
    async fn an_account_whose_listing_fails_is_one_unusable_host_and_hides_no_other_account() {
        // Given Work's deployment rejecting its token, and Lab's listing one workspace
        let deployments = coder_deployments()
            .failing(
                WORK_URL,
                CoderWorkspaceListError::Rejected("API key is invalid.".to_string()),
            )
            .listing(LAB_URL, vec![a_coder_workspace("ada", "gpu", "running")]);

        // When
        let listed = listed_for_ada(adas_vault(), deployments).await;

        // Then Work is one host that cannot be used, saying why and targeting nothing
        assert_eq!(
            listed,
            Ok(vec![
                DerivedHost {
                    id: format!("{PARENT}/coder/{WORK}"),
                    label: "Work".to_string(),
                    kind: DerivedHostKind::Coder as i32,
                    parent_instance_id: PARENT.to_string(),
                    target: None,
                    usable: false,
                    unusable_reason:
                        "the Coder deployment rejected this account's token: API key is invalid."
                            .to_string(),
                },
                a_usable_host(LAB, "Lab", "gpu"),
            ])
        );
    }

    #[test]
    fn a_daemon_started_without_coder_accounts_lists_nobody_any() {
        assert_eq!(NoCoderAccounts.accounts_of(ADA), Ok(Vec::new()));
    }
}

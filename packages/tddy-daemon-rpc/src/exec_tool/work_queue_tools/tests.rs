use pretty_assertions::assert_eq;
use tddy_changeset::work_queue_fixtures::{a_stack_host, A_PROJECT};
use tddy_core::changeset::{Changeset, WorkItem, WorkQueue};

use super::ops;
use super::*;

// ── Reads through an account that predates the provider identifiers ────────────────

use std::sync::Arc;

use tddy_credentials::{
    AccountId, CredentialRecord, CredentialStore, ProviderId, SecretString, SessionVaults,
    FIRST_VERSION,
};
use tddy_github::pr_api::{
    CheckRun, CheckStatus, PrDetail, PrFile, PrIssueComment, PrLookupOutcome, PrRef, PrReview,
    PrReviewComment, PrSearchHit, PrSearchQuery, PrState,
};
use tddy_projects::project_storage::{
    write_projects, AccountAssignment, ProjectData, ProjectLifecycle,
};
use tddy_tool_engine::work_queue_tools::PrAddress;

/// Ada's account as a row linked before the provider identifiers existed: a token and a label,
/// no `subject_id`, no `subject`.
fn a_github_account_without_identifiers() -> CredentialRecord {
    CredentialRecord {
        provider: ProviderId::new("github"),
        account: AccountId::new("acct-ada"),
        label: "Ada's work account".to_string(),
        secret: SecretString::new("ghp_ada_token"),
        metadata: Default::default(),
        updated_at: 1_700_000_000,
        version: FIRST_VERSION,
    }
}

/// Vaults holding Ada's identifier-less account, unlocked; and access to them as her session.
fn access_to_ada_s_vault_without_identifiers() -> (tempfile::TempDir, SessionAccountAccess) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let passphrase = SecretString::new("correct horse battery staple");
    let store = CredentialStore::create(
        &CredentialStore::path_in(dir.path(), "ada"),
        &passphrase,
        "ada",
    )
    .expect("Ada's vault is created");
    store
        .put(a_github_account_without_identifiers())
        .expect("the record is retained");
    let vaults = Arc::new(SessionVaults::new(dir.path()));
    vaults
        .unlock("ada", &passphrase)
        .expect("Ada's passphrase opens her vault");
    let access = SessionAccountAccess::new(
        Some(vaults),
        Arc::new(|token: &str| (token == "session-token-for-ada").then(|| "ada".to_string())),
        "session-token-for-ada",
    );
    (dir, access)
}

/// GitHub serving pull request 7, and nothing else.
#[derive(Clone)]
struct APrSeven;

fn pr_seven() -> PrDetail {
    PrDetail {
        number: 7,
        url: "https://github.com/acme/alpha/pull/7".to_string(),
        title: "Add the thing".to_string(),
        body: "It adds the thing.".to_string(),
        state: PrState::Open,
        base_branch: "main".to_string(),
        head_branch: "feature/alpha/n1".to_string(),
        head_sha: "abc123".to_string(),
        mergeable: Some(true),
        mergeable_state: "clean".to_string(),
        additions: 10,
        deletions: 2,
        changed_files: 1,
    }
}

impl WorkQueueGithubClients for APrSeven {
    fn client_for(&self, _repo: &str, _token: &str) -> Box<dyn WorkQueueGithub> {
        Box::new(self.clone())
    }
}

fn unserved(what: &str) -> tddy_core::WorkflowError {
    tddy_core::WorkflowError::WriteFailed(format!("the stand-in GitHub serves no {what}"))
}

impl GithubPrApi for APrSeven {
    fn get_open_pr(&self, _head_branch: &str) -> Result<Option<PrRef>, tddy_core::WorkflowError> {
        Ok(None)
    }
    fn get_pr_by_head(&self, _head_branch: &str) -> PrLookupOutcome {
        PrLookupOutcome::NotFound
    }
    fn merge_pr(&self, _number: u64) -> Result<String, tddy_core::WorkflowError> {
        Err(unserved("merge"))
    }
    fn patch_pr_base(&self, _number: u64, _new_base: &str) -> Result<(), tddy_core::WorkflowError> {
        Err(unserved("repoint"))
    }
    fn create_pr(
        &self,
        _head: &str,
        _base: &str,
        _title: &str,
        _body: &str,
    ) -> Result<u64, tddy_core::WorkflowError> {
        Err(unserved("PR creation"))
    }
    fn disable_auto_merge(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Err(unserved("auto-merge"))
    }
    fn close_pr(&self, _number: u64) -> Result<(), tddy_core::WorkflowError> {
        Err(unserved("close"))
    }
}

impl GithubPrInsightApi for APrSeven {
    fn get_pr(&self, number: u64) -> Result<PrDetail, tddy_core::WorkflowError> {
        if number == 7 {
            Ok(pr_seven())
        } else {
            Err(unserved("that PR"))
        }
    }
    fn list_pr_files(&self, _number: u64) -> Result<Vec<PrFile>, tddy_core::WorkflowError> {
        Ok(Vec::new())
    }
    fn list_check_runs(&self, _head_sha: &str) -> Result<Vec<CheckRun>, tddy_core::WorkflowError> {
        Ok(vec![CheckRun {
            id: 1,
            name: "build".to_string(),
            status: CheckStatus::Completed,
            conclusion: "success".to_string(),
            started_at: None,
            completed_at: None,
            details_url: String::new(),
        }])
    }
    fn list_reviews(&self, _number: u64) -> Result<Vec<PrReview>, tddy_core::WorkflowError> {
        Ok(Vec::new())
    }
    fn list_review_comments(
        &self,
        _number: u64,
    ) -> Result<Vec<PrReviewComment>, tddy_core::WorkflowError> {
        Ok(Vec::new())
    }
    fn list_issue_comments(
        &self,
        _number: u64,
    ) -> Result<Vec<PrIssueComment>, tddy_core::WorkflowError> {
        Ok(Vec::new())
    }
    fn search_prs(
        &self,
        _query: &PrSearchQuery,
    ) -> Result<Vec<PrSearchHit>, tddy_core::WorkflowError> {
        Ok(Vec::new())
    }
    fn patch_pr_title_body(
        &self,
        _number: u64,
        _title: Option<&str>,
        _body: Option<&str>,
    ) -> Result<(), tddy_core::WorkflowError> {
        Err(unserved("title/body edit"))
    }
}

/// A host whose project is vault-backed (not Local gh), assigned to Ada's account, and whose
/// fixture queue `owner` owns.
struct VaultBackedHost {
    host: tddy_changeset::work_queue_fixtures::AStackHost,
    _vault_dir: tempfile::TempDir,
    access: SessionAccountAccess,
}

fn a_vault_backed_host() -> VaultBackedHost {
    let host = a_stack_host();
    host.orchestrator("owner", a_queue_linking(None));
    let (_vault_dir, access) = access_to_ada_s_vault_without_identifiers();
    write_projects(
        &host.projects_dir(),
        &[ProjectData {
            project_id: A_PROJECT.to_string(),
            name: "Alpha".to_string(),
            git_url: "https://github.com/acme/alpha.git".to_string(),
            main_repo_path: host.base().join("repos").display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: Default::default(),
            accounts: vec![AccountAssignment {
                provider: "github".to_string(),
                account_id: "acct-ada".to_string(),
            }],
            local_gh: false,
            home_session_id: None,
            orchestrator_defaults: None,
            lifecycle: ProjectLifecycle::default(),
        }],
    )
    .expect("the project row is written");
    VaultBackedHost {
        host,
        _vault_dir,
        access,
    }
}

impl VaultBackedHost {
    fn github(&self) -> GithubSource {
        GithubSource {
            projects_dir: self.host.projects_dir(),
            project_id: A_PROJECT.to_string(),
            os_user: "somebody".to_string(),
            identity: self.access.clone(),
            github_cli: GithubCli::on_spawn_child_path(),
            clients: Arc::new(APrSeven),
        }
    }

    fn env(&self, queue_id: Option<&str>) -> ops::OpEnv {
        ops::OpEnv {
            store: self.host.store(),
            project_id: A_PROJECT.to_string(),
            queue_id: queue_id.map(str::to_string),
            sessions_base: self.host.base().to_path_buf(),
        }
    }
}

#[test]
fn a_pr_read_through_an_account_without_identifiers_answers() {
    // Given a vault-backed project assigned to Ada's identifier-less account
    let world = a_vault_backed_host();

    // When work_pr_read reads pull request 7 by number
    let answer = ops::run(
        &world.env(None),
        &world.github(),
        WorkQueueOp::ReadPr {
            pr: PrAddress::Number(7),
            include_files: false,
        },
    )
    .expect("a read needs no commit identity");

    // Then it is PR 7's view
    assert_eq!(answer["number"], serde_json::json!(7));
    assert_eq!(answer["title"], serde_json::json!("Add the thing"));
}

#[test]
fn assigning_a_pr_through_an_account_without_identifiers_records_it() {
    // Given a vault-backed project assigned to Ada's identifier-less account, and n1 unassigned
    let world = a_vault_backed_host();

    // When work_item_assign_pr assigns pull request 7 to n1
    ops::run(
        &world.env(Some("owner")),
        &world.github(),
        WorkQueueOp::AssignPr {
            item_id: "n1".to_string(),
            pull_number: 7,
        },
    )
    .expect("an assign needs no commit identity");

    // Then n1 records PR 7's own URL and head branch — never refused for want of an identity
    let node = world
        .host
        .store()
        .read(A_PROJECT, "owner")
        .expect("the queue reads")
        .stack
        .node("n1")
        .expect("n1 is planned")
        .clone();
    let assigned = node.pr.expect("n1 records its PR");
    assert_eq!(assigned.number, 7);
    assert_eq!(
        assigned.url,
        "https://github.com/acme/alpha/pull/7".to_string()
    );
    assert_eq!(assigned.head_branch, "feature/alpha/n1".to_string());
}

fn a_queue_linking(child: Option<&str>) -> WorkQueue {
    WorkQueue {
        version: 1,
        nodes: vec![WorkItem {
            node_id: "n1".to_string(),
            title: "Store".to_string(),
            session_id: child.map(str::to_string),
            ..WorkItem::default()
        }],
        ..Default::default()
    }
}

#[test]
fn a_session_owning_a_queue_is_its_owner_whatever_its_role() {
    // Given a session with no orchestrator role that owns a queue
    let host = a_stack_host();
    host.orchestrator("owner", a_queue_linking(None));

    // When it is read
    let caller = work_queue_caller_of(host.base(), "owner").unwrap();

    // Then it is the owner of its project's queue
    assert_eq!(
        caller,
        Some((
            WorkQueueCaller {
                session_id: "owner".to_string(),
                project_id: A_PROJECT.to_string(),
            },
            WorkQueueAccess::Owner
        ))
    );
}

#[test]
fn an_orchestrator_role_session_owning_no_queue_has_no_access() {
    // Given a project-home session that owns no queue (access never reads the role)
    let host = a_stack_host();
    let dir = host.session("main", Changeset::default());
    let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
    meta.session_role = Some("project-home".to_string());
    tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();

    // When it is read
    let caller = work_queue_caller_of(host.base(), "main").unwrap();

    // Then it has none
    assert_eq!(caller, None);
}

#[test]
fn a_session_with_no_record_is_refused_rather_than_read_as_ordinary() {
    // Given a session id with nothing on disk
    let host = a_stack_host();

    // When it is read
    let refusal =
        work_queue_caller_of(host.base(), "ghost").expect_err("its access cannot be known");

    // Then it is refused as permission denied, naming the session
    assert_eq!(refusal.code(), tddy_rpc::Code::PermissionDenied);
    assert!(refusal.message().contains("ghost"), "{}", refusal.message());
}

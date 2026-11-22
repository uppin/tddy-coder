//! A session's PR view through the daemon's GitHub inbox (`#workqueue` 4/14, the inspector's PR tab),
//! against a scripted GitHub. These exercise n2's inbox behaviour on purpose — reading through it is
//! the point — so they go green once n2 is green.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_github::conditional::{ConditionalAnswer, ConditionalRead, GithubReadError};
use tddy_github_inbox::{paths, GithubInbox, InboxError, RepoReaders, RepoSlug};
use tddy_rpc::{Request, Status};
use tddy_service::proto::session::SessionEntry;
use tddy_service::proto::worktree::{SessionWorktreeRequest, WorktreeService};
use tddy_worktree_service::pull_request_view::{
    apply_cached_pull_request, pull_request_summary, pull_request_view, watch_pull_request,
    PullRequestSite, PullRequestSummary, SessionPullRequests,
};
use tddy_worktree_service::test_util::{current_os_user, test_service, TEST_TOKEN};
use tokio_stream::StreamExt;

const T0: u64 = 1_795_000_000_000;
const A_MINUTE: u64 = 60_000;

fn the_repo() -> RepoSlug {
    RepoSlug {
        owner: "acme".to_string(),
        name: "app".to_string(),
    }
}

#[derive(Default)]
struct AScriptedGithub {
    answers: Mutex<HashMap<String, VecDeque<ConditionalAnswer>>>,
    requests: Mutex<Vec<String>>,
}

impl AScriptedGithub {
    fn answering(self, path: String, body: impl Into<String>) -> Self {
        self.answers
            .lock()
            .unwrap()
            .entry(path.clone())
            .or_default()
            .push_back(ConditionalAnswer::Modified {
                body: body.into(),
                etag: Some(format!("etag-{path}")),
                poll_interval_ms: Some(A_MINUTE),
                rate_limit: None,
            });
        self
    }

    fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

#[async_trait]
impl ConditionalRead for AScriptedGithub {
    async fn get(
        &self,
        path: &str,
        _if_none_match: Option<&str>,
    ) -> Result<ConditionalAnswer, GithubReadError> {
        self.requests.lock().unwrap().push(path.to_string());
        let mut answers = self.answers.lock().unwrap();
        let queue = answers
            .get_mut(path)
            .unwrap_or_else(|| panic!("unscripted request to {path}"));
        Ok(if queue.len() > 1 {
            queue.pop_front().unwrap()
        } else {
            queue.front().cloned().unwrap()
        })
    }
}

struct ReadsThrough(Arc<AScriptedGithub>);

impl RepoReaders for ReadsThrough {
    fn reader_for(&self, _repo: &RepoSlug) -> Result<Arc<dyn ConditionalRead>, InboxError> {
        Ok(self.0.clone())
    }
}

fn an_inbox_over(github: &Arc<AScriptedGithub>) -> Arc<GithubInbox> {
    Arc::new(GithubInbox::new(Arc::new(ReadsThrough(github.clone()))))
}

fn a_pull_json(number: u64, head: &str, head_sha: &str) -> String {
    format!(
        r#"{{"number":{number},"html_url":"https://github.com/acme/app/pull/{number}","title":"Add the thing","body":"","state":"open","draft":false,"base":{{"ref":"master"}},"head":{{"ref":"{head}","sha":"{head_sha}"}},"mergeable":true,"mergeable_state":"clean","additions":1,"deletions":0,"changed_files":1}}"#
    )
}

fn check_runs_json(status: &str, conclusion: Option<&str>) -> String {
    let conclusion = conclusion.map_or("null".to_string(), |c| format!("\"{c}\""));
    format!(
        r#"{{"total_count":1,"check_runs":[{{"id":1,"name":"build","status":"{status}","conclusion":{conclusion},"started_at":"2026-11-22T10:00:00Z","completed_at":null,"details_url":"https://github.com/acme/app/runs/1"}}]}}"#
    )
}

/// A GitHub where `feat/one`'s PR #7 has a running build, one approving review, one review comment
/// with a reply, one conversation comment and two commits.
fn a_github_with_one_pr() -> AScriptedGithub {
    let repo = the_repo();
    AScriptedGithub::default()
        .answering(
            paths::pulls_for_head(&repo, "feat/one"),
            format!("[{}]", a_pull_json(7, "feat/one", "sha-head")),
        )
        .answering(paths::pull(&repo, 7), a_pull_json(7, "feat/one", "sha-head"))
        .answering(paths::check_runs(&repo, "sha-head"), check_runs_json("in_progress", None))
        .answering(
            paths::reviews(&repo, 7),
            r#"[{"id":31,"user":{"login":"rev"},"state":"APPROVED","body":"lgtm","submitted_at":"2026-11-22T11:00:00Z"}]"#,
        )
        .answering(
            paths::review_comments(&repo, 7),
            r#"[{"id":41,"in_reply_to_id":null,"user":{"login":"rev"},"body":"why?","path":"a.rs","line":3,"diff_hunk":"@@","created_at":"2026-11-22T11:01:00Z"},
                {"id":42,"in_reply_to_id":41,"user":{"login":"dev"},"body":"because","path":"a.rs","line":3,"diff_hunk":"@@","created_at":"2026-11-22T11:02:00Z"}]"#,
        )
        .answering(
            paths::issue_comments(&repo, 7),
            r#"[{"id":51,"user":{"login":"ops"},"body":"ship it","created_at":"2026-11-22T11:03:00Z"}]"#,
        )
        .answering(
            paths::pr_commits(&repo, 7),
            r#"[{"sha":"sha-a","commit":{"author":{"name":"dev","date":"2026-11-22T09:00:00Z"},"message":"first\n"}},
                {"sha":"sha-head","commit":{"author":{"name":"dev","date":"2026-11-22T09:30:00Z"},"message":"second\n"}}]"#,
        )
}

#[tokio::test]
async fn the_view_carries_the_pr_its_checks_conversation_and_commits() {
    // Given
    let github = Arc::new(a_github_with_one_pr());
    let inbox = an_inbox_over(&github);

    // When
    let view = pull_request_view(&inbox, &the_repo(), "feat/one")
        .await
        .expect("the view");

    // Then
    assert_eq!(
        (
            view.exists,
            view.number,
            view.state.as_str(),
            view.checks
                .iter()
                .map(|c| (c.name.as_str(), c.status.as_str()))
                .collect::<Vec<_>>(),
            view.comments
                .iter()
                .map(|c| (c.id, c.in_reply_to_id))
                .collect::<Vec<_>>(),
            view.commits
                .iter()
                .map(|c| c.subject.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            true,
            7,
            "open",
            vec![("build", "in_progress")],
            vec![(31, 0), (41, 0), (42, 41), (51, 0)],
            vec!["first", "second"],
        )
    );
}

#[tokio::test]
async fn a_branch_without_a_pr_is_a_view_that_does_not_exist() {
    // Given
    let repo = the_repo();
    let github = Arc::new(
        AScriptedGithub::default().answering(paths::pulls_for_head(&repo, "feat/none"), "[]"),
    );
    let inbox = an_inbox_over(&github);

    // When
    let view = pull_request_view(&inbox, &repo, "feat/none")
        .await
        .expect("the view");

    // Then
    assert!(!view.exists, "no PR, no view: {view:?}");
}

#[tokio::test]
async fn a_cold_branch_has_no_summary_and_makes_no_request() {
    // Given
    let github = Arc::new(a_github_with_one_pr());
    let inbox = an_inbox_over(&github);

    // When
    let summary = pull_request_summary(&inbox, &the_repo(), "feat/one");

    // Then
    assert_eq!((summary, github.request_count()), (None, 0));
}

#[tokio::test]
async fn a_cached_branch_summarises_its_pr_number_and_state() {
    // Given — the inbox has read the branch's PR once
    let github = Arc::new(a_github_with_one_pr());
    let inbox = an_inbox_over(&github);
    inbox
        .pr_for_head(&the_repo(), "feat/one")
        .await
        .expect("pr for head");

    // When
    let summary = pull_request_summary(&inbox, &the_repo(), "feat/one");

    // Then
    assert_eq!(
        summary,
        Some(PullRequestSummary {
            number: 7,
            state: "open".to_string()
        })
    );
}

#[tokio::test]
async fn a_completed_check_reaches_an_open_watch() {
    // Given — a watch on a PR whose build is running
    let repo = the_repo();
    let github = Arc::new(
        a_github_with_one_pr()
            .answering(
                paths::check_runs(&repo, "sha-head"),
                check_runs_json("completed", Some("success")),
            )
            .answering(paths::events(&repo), "[]"),
    );
    let inbox = an_inbox_over(&github);
    let mut watch = watch_pull_request(inbox.clone(), repo.clone(), "feat/one".to_string());
    let first = watch.next().await.expect("the first view");

    // When — the inbox's pending-check poll sees the build finish
    inbox.step(T0).await;
    let next = watch.next().await.expect("a view after the change");

    // Then
    assert_eq!(
        (
            first.checks[0].status.as_str(),
            next.checks[0].status.as_str(),
            next.checks[0].conclusion.as_str()
        ),
        ("in_progress", "completed", "success")
    );
}

/// The host's side of the PR port, over an inbox the test scripted: every project is `acme/app`.
struct ProjectsOfAcmeApp(Arc<GithubInbox>);

impl SessionPullRequests for ProjectsOfAcmeApp {
    fn site_for_session(
        &self,
        _os_user: &str,
        _session_id: &str,
        _project_id: &str,
        _session_token: &str,
    ) -> Result<PullRequestSite, Status> {
        Ok(PullRequestSite {
            inbox: self.0.clone(),
            repo: the_repo(),
        })
    }

    fn cached_summary(
        &self,
        _os_user: &str,
        _project_id: &str,
        branch: &str,
    ) -> Option<PullRequestSummary> {
        pull_request_summary(&self.0, &the_repo(), branch)
    }
}

/// A session directory under `data_dir` whose changeset records `branch`.
fn a_session_on_branch(data_dir: &std::path::Path, session_id: &str, branch: &str) {
    let session_dir = data_dir.join("sessions").join(session_id);
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join(tddy_core::SESSION_METADATA_FILENAME),
        format!(
            "session_id: {session_id}\nproject_id: proj-1\ncreated_at: '2026-11-22T10:00:00Z'\nupdated_at: '2026-11-22T10:00:00Z'\nstatus: active\n"
        ),
    )
    .unwrap();
    tddy_core::write_changeset(
        &session_dir,
        &tddy_core::Changeset {
            branch: Some(branch.to_string()),
            ..Default::default()
        },
    )
    .unwrap();
}

#[tokio::test]
async fn the_stream_sends_the_view_of_the_pr_on_the_sessions_branch() {
    // Given — a session on `feat/one`, whose PR the daemon's inbox can read
    let data_dir = tempfile::tempdir().unwrap();
    a_session_on_branch(data_dir.path(), "sess-1", "feat/one");
    let github = Arc::new(a_github_with_one_pr());
    let service = test_service(data_dir.path().to_path_buf(), &current_os_user());
    service.attach_pull_requests(Arc::new(ProjectsOfAcmeApp(an_inbox_over(&github))));

    // When
    let mut stream = service
        .stream_session_pull_request(Request::direct(SessionWorktreeRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: "sess-1".to_string(),
            ..Default::default()
        }))
        .await
        .expect("the stream")
        .into_inner();
    let first = stream
        .next()
        .await
        .expect("a first view")
        .expect("an ok view");

    // Then
    assert_eq!(
        (first.exists, first.number, first.head_branch.as_str()),
        (true, 7, "feat/one")
    );
}

#[tokio::test]
async fn a_listed_session_carries_the_cached_pr_of_its_branch() {
    // Given — the inbox has read `feat/one`'s PR, and nothing of `feat/none`
    let repo = the_repo();
    let github =
        Arc::new(a_github_with_one_pr().answering(paths::pulls_for_head(&repo, "feat/none"), "[]"));
    let inbox = an_inbox_over(&github);
    inbox
        .pr_for_head(&repo, "feat/one")
        .await
        .expect("pr for head");
    let port = ProjectsOfAcmeApp(inbox);
    let listed = |branch: &str| SessionEntry {
        project_id: "proj-1".to_string(),
        branch: branch.to_string(),
        ..Default::default()
    };
    let (mut with_pr, mut without_pr) = (listed("feat/one"), listed("feat/none"));

    // When
    apply_cached_pull_request(&port, "os-user", &mut with_pr);
    apply_cached_pull_request(&port, "os-user", &mut without_pr);

    // Then
    assert_eq!(
        (
            with_pr.pr_number,
            with_pr.pr_state.as_str(),
            without_pr.pr_number,
            without_pr.pr_state.as_str()
        ),
        (7, "open", 0, "")
    );
}

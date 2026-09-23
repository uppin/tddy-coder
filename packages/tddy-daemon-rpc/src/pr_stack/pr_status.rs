//! The PR leg of a branch resolution, and the base-sync leg beside it, read with the calling
//! operator's own GitHub credential.

use super::branch_legs::{base_sync_through_cache, base_sync_unavailable, base_sync_view};
use super::PrStackRpcHandler;
use tddy_session_lifecycle::connection_service::spawn_blocking_with_timeout;

impl PrStackRpcHandler {
    /// PR status for one branch, resolved with the calling operator's own GitHub credential.
    ///
    /// `repo_root` is `None` when no file in the session directory records a checkout (see
    /// [`tddy_core::repo_root_for_session`]) — an unknown repository, not a repository without PRs.
    ///
    /// Never fails: a lookup that cannot be performed degrades this one field to *unavailable* (D8),
    /// and stub/demo authentication resolves to an empty result (D12) — so the enclosing RPC keeps
    /// returning its other legs instead of collapsing into an error the web discards wholesale.
    pub(crate) async fn pr_status_for_caller(
        &self,
        github_login: &str,
        repo_root: Option<&std::path::Path>,
        branch: &str,
    ) -> tddy_service::proto::pr_stack::PrStatusView {
        use tddy_service::proto::pr_stack::PrStatusView;
        use tddy_session_lifecycle::github_pr_credentials::{pr_lookup_for_caller, PrLookup};
        use tddy_workflow_recipes::orchestrate_pr_stack::github::PrLookupOutcome;

        // Both ways of failing to name a GitHub repository leave the lookup un-performable, so they
        // are *unavailable* with a reason — reporting `exists = false` would claim the branch has no
        // PR when in fact nothing was ever asked (D8).
        let Some(repo_root) = repo_root else {
            return pr_status_unavailable(
                branch,
                "no checkout is recorded for this session, so its GitHub repository is unknown"
                    .to_string(),
            );
        };
        let Some(owner_repo) = owner_repo_from_repo_root(repo_root) else {
            return pr_status_unavailable(
                branch,
                format!(
                    "no GitHub repository could be resolved from the origin remote of {}",
                    repo_root.display()
                ),
            );
        };
        let stub_mode = self
            .config
            .github
            .as_ref()
            .and_then(|g| g.stub)
            .unwrap_or(false);
        let stored = self
            .github_token_store
            .as_ref()
            .and_then(|store| store.get(github_login));
        let token = match pr_lookup_for_caller(stub_mode, stored.as_deref()) {
            PrLookup::Empty => return PrStatusView::default(),
            PrLookup::Unavailable(reason) => return pr_status_unavailable(branch, reason),
            PrLookup::Perform(token) => token,
        };

        let head_branch = branch.to_string();
        let outcome = tokio::task::spawn_blocking(move || {
            use tddy_workflow_recipes::orchestrate_pr_stack::github::GithubPrApi;
            tddy_workflow_recipes::orchestrate_pr_stack::github::RealGithubPrApi::with_token(
                owner_repo, token,
            )
            .get_pr_by_head(&head_branch)
        })
        .await;

        match outcome {
            Ok(PrLookupOutcome::Found(pr)) => PrStatusView {
                exists: true,
                number: pr.number,
                url: pr.url,
                state: pr_state_label(pr.state).to_string(),
                unavailable: false,
                unavailable_reason: String::new(),
            },
            Ok(PrLookupOutcome::NotFound) => PrStatusView::default(),
            Ok(PrLookupOutcome::Unavailable(reason)) => pr_status_unavailable(branch, reason),
            Err(join_error) => pr_status_unavailable(
                branch,
                format!("the PR lookup did not complete: {join_error}"),
            ),
        }
    }

    /// How the branch stands against the base the caller named, for `QueryBranch`'s fifth leg.
    ///
    /// Never fails, exactly like the session, worktree, remote and PR legs beside it: an unnamed
    /// base, an unknown checkout, a probe that could not run and a probe that ran out of time all
    /// arrive as `unavailable` carrying a reason. A comparison that could not be made is byte-
    /// identical to a healthy one on every other field, so the discriminator is the only thing that
    /// keeps "could not tell" from rendering as "clean" (PRD D27).
    ///
    /// An unnamed base is reported unavailable rather than substituted with the project default
    /// (D29): this is a display, and the number beside a row must describe the same base the row's
    /// own base line shows.
    pub(crate) async fn base_sync_leg(
        &self,
        repo_root: Option<&std::path::Path>,
        branch: &str,
        base_branch: &str,
    ) -> Option<tddy_service::proto::pr_stack::BranchBaseSync> {
        if base_branch.is_empty() {
            return Some(base_sync_unavailable(
                "",
                "no base branch was named for this branch, so there is nothing to compare it \
                 against",
            ));
        }
        let Some(repo_root) = repo_root else {
            return Some(base_sync_unavailable(
                base_branch,
                "no checkout is recorded for this session, so its repository could not be resolved",
            ));
        };

        let probe_root = repo_root.to_path_buf();
        let probe_branch = branch.to_string();
        let probe_base = base_branch.to_string();
        let probed = spawn_blocking_with_timeout(
            self.config.spawn_worker_request_timeout(),
            "QueryBranch: compare branch against base",
            move || {
                Ok(base_sync_through_cache(
                    &probe_root,
                    &probe_branch,
                    &probe_base,
                ))
            },
        )
        .await;

        Some(match probed {
            Ok(Ok(sync)) => base_sync_view(sync),
            Ok(Err(reason)) => base_sync_unavailable(base_branch, &reason),
            // A timeout degrades this leg; it must not take the other four with it.
            Err(status) => base_sync_unavailable(
                base_branch,
                &format!("the comparison did not complete: {}", status.message()),
            ),
        })
    }
}

/// Derive `owner/repo` from a repo's `origin` remote URL, for GitHub API namespacing.
/// Returns `None` when the remote can't be read or isn't a recognizable GitHub URL.
pub(crate) fn owner_repo_from_repo_root(repo_root: &std::path::Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let remote_url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    tddy_workflow_recipes::orchestrate_pr_stack::github::owner_repo_from_remote_url(&remote_url)
}

/// A PR status the daemon could not look up: *unavailable* with an operator-facing `reason`, never
/// `exists = false` (D8). Logged, because a lookup that never happened is otherwise invisible — the
/// daemon log carried no PR line at all for an orchestrator polled hundreds of times.
fn pr_status_unavailable(
    branch: &str,
    reason: String,
) -> tddy_service::proto::pr_stack::PrStatusView {
    log::warn!("PR status unavailable for branch {branch}: {reason}");
    tddy_service::proto::pr_stack::PrStatusView {
        unavailable: true,
        unavailable_reason: reason,
        ..Default::default()
    }
}

/// GitHub PR state → the lowercase label carried on the `PrStatusView.state` wire field.
fn pr_state_label(
    state: tddy_workflow_recipes::orchestrate_pr_stack::github::PrState,
) -> &'static str {
    use tddy_workflow_recipes::orchestrate_pr_stack::github::PrState;
    match state {
        PrState::Open => "open",
        PrState::Merged => "merged",
        PrState::Closed => "closed",
        PrState::Draft => "draft",
    }
}

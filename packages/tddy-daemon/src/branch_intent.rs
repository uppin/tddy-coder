//! The one place a `StartSession` request's branch fields become a changeset workflow.
//!
//! Every session type that owns a git worktree resolves the same four request fields —
//! `branch_worktree_intent`, `new_branch_name`, `selected_integration_base_ref`,
//! `selected_branch_to_work_on` — into the [`ChangesetWorkflow`] the worktree setup reads. They once
//! did it with a copy of the same match each, and the copies drifted in ways the new-session form
//! cannot show: a workspace session is the *codebase half* of a split claude-cli session
//! (`docs/ft/daemon/remote-managed-worktree.md`), so the same form, submitted once, was resolved by
//! two different copies depending on where the codebase was placed — and a fix to one that missed
//! the other would come up on a differently named branch than the one that was picked.
//!
//! What legitimately differs between spawn paths is captured in [`BranchIntentPolicy`]: the prefix a
//! generated branch name carries, and what to do with a request that names nothing usable. Nothing
//! else may.

use tddy_core::{BranchWorktreeIntent, ChangesetWorkflow};
use tddy_rpc::Status;

/// The branch fields of a `StartSessionRequest`, as handed to a spawn path.
#[derive(Debug, Default, Clone, Copy)]
pub struct BranchIntentRequest<'a> {
    pub branch_worktree_intent: &'a str,
    pub new_branch_name: &'a str,
    pub selected_integration_base_ref: &'a str,
    pub selected_branch_to_work_on: &'a str,
}

/// What a spawn path does with a `new_branch_from_base` that names no branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlankNewBranchName {
    /// Refuse: the caller asked for a specific branch and then did not name one, which is a
    /// malformed request rather than a request for a generated name.
    Refuse,
    /// Generate one from [`BranchIntentPolicy::generated_branch_prefix`], as an omitted intent does.
    Generate,
}

/// What a spawn path does with an intent string it does not recognise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnrecognizedIntent {
    /// Refuse, naming the offending value. A branch was chosen in the new-session form, and coming
    /// up on a generated branch instead looks exactly like the session that was asked for.
    Refuse,
    /// Treat it as an omitted intent: a new branch from the base under a generated name. The
    /// pre-existing behaviour of the claude-cli and cursor-cli paths, kept because clients that
    /// send an unrecognised value today get a session, and refusing them is a behaviour change that
    /// belongs to those paths rather than to this extraction.
    DefaultToGeneratedBranch,
}

/// How one spawn path resolves the parts of a branch intent that are genuinely its own.
#[derive(Debug, Clone, Copy)]
pub struct BranchIntentPolicy {
    /// Leading path segment of a generated branch name (`<prefix>/<short session id>`), so an
    /// operator reading `git branch` can tell what created a branch.
    pub generated_branch_prefix: &'static str,
    pub blank_new_branch_name: BlankNewBranchName,
    pub unrecognized_intent: UnrecognizedIntent,
}

impl BranchIntentPolicy {
    /// claude-cli, co-located or sandboxed: a named intent must name its branch.
    pub const fn claude_cli() -> Self {
        Self {
            generated_branch_prefix: "claude-cli",
            blank_new_branch_name: BlankNewBranchName::Refuse,
            unrecognized_intent: UnrecognizedIntent::DefaultToGeneratedBranch,
        }
    }

    /// cursor-cli, co-located or sandboxed: a `new_branch_from_base` with no name generates one.
    pub const fn cursor_cli() -> Self {
        Self {
            generated_branch_prefix: "cursor-cli",
            blank_new_branch_name: BlankNewBranchName::Generate,
            unrecognized_intent: UnrecognizedIntent::DefaultToGeneratedBranch,
        }
    }

    /// A `workspace` session — including the codebase half of a split session, where the intent was
    /// forwarded from another daemon and a silent reinterpretation of it would be invisible on both
    /// hosts.
    pub const fn workspace() -> Self {
        Self {
            generated_branch_prefix: "workspace",
            blank_new_branch_name: BlankNewBranchName::Refuse,
            unrecognized_intent: UnrecognizedIntent::Refuse,
        }
    }
}

/// A resolved branch intent: the workflow to persist, and the intent itself.
///
/// The intent is returned beside the workflow because callers act on it after writing the changeset
/// — pushing a freshly created branch to origin happens only under `NewBranchFromBase` — and reading
/// it back out of an `Option` field would mean handling a `None` that cannot occur.
#[derive(Debug)]
pub struct ResolvedBranchWorkflow {
    pub intent: BranchWorktreeIntent,
    pub workflow: ChangesetWorkflow,
}

/// Resolve `request` into the changeset workflow the worktree setup reads.
///
/// `project_main_branch_ref` is the project's stored default branch, or `None` for a spawn with no
/// registered project to read one from. It is consulted only for a new branch with no explicit base:
/// an explicit `selected_integration_base_ref` always wins, and a project without a stored default
/// leaves the base `None` so the worktree setup resolves it live.
pub fn resolve_branch_workflow(
    session_id: &str,
    request: &BranchIntentRequest<'_>,
    policy: BranchIntentPolicy,
    project_main_branch_ref: Option<&str>,
) -> Result<ResolvedBranchWorkflow, Status> {
    let generated_branch = || {
        format!(
            "{}/{}",
            policy.generated_branch_prefix,
            &session_id[..8.min(session_id.len())]
        )
    };

    let (intent, new_branch_name, selected_branch_to_work_on) =
        match request.branch_worktree_intent.trim() {
            "new_branch_from_base" => {
                let named = request.new_branch_name.trim();
                let branch = match (named.is_empty(), policy.blank_new_branch_name) {
                    (true, BlankNewBranchName::Refuse) => {
                        return Err(Status::invalid_argument(
                        "new_branch_name is required when branch_worktree_intent is new_branch_from_base",
                    ))
                    }
                    (true, BlankNewBranchName::Generate) => generated_branch(),
                    (false, _) => named.to_string(),
                };
                (BranchWorktreeIntent::NewBranchFromBase, Some(branch), None)
            }
            "work_on_selected_branch" => {
                let selected = request.selected_branch_to_work_on.trim();
                if selected.is_empty() {
                    return Err(Status::invalid_argument(
                    "selected_branch_to_work_on is required when branch_worktree_intent is work_on_selected_branch",
                ));
                }
                (
                    BranchWorktreeIntent::WorkOnSelectedBranch,
                    None,
                    Some(selected.to_string()),
                )
            }
            // An omitted intent is the documented default (`StartSessionRequest`): a new branch
            // from the base, named after the session.
            "" => (
                BranchWorktreeIntent::NewBranchFromBase,
                Some(generated_branch()),
                None,
            ),
            other => match policy.unrecognized_intent {
                UnrecognizedIntent::Refuse => {
                    return Err(Status::invalid_argument(format!(
                        "unrecognized branch_worktree_intent {other:?}: expected \"new_branch_from_base\", \"work_on_selected_branch\", or empty"
                    )))
                }
                UnrecognizedIntent::DefaultToGeneratedBranch => (
                    BranchWorktreeIntent::NewBranchFromBase,
                    Some(generated_branch()),
                    None,
                ),
            },
        };

    // An explicit client override wins; otherwise a new branch is cut from the project's stored
    // default. Without this a project with a configured default branch would get a worktree off a
    // different base for no reason other than which spawn path served the request.
    let selected_integration_base_ref = match request.selected_integration_base_ref.trim() {
        "" if matches!(intent, BranchWorktreeIntent::NewBranchFromBase) => project_main_branch_ref
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        "" => None,
        explicit => Some(explicit.to_string()),
    };

    Ok(ResolvedBranchWorkflow {
        intent,
        workflow: ChangesetWorkflow {
            branch_worktree_intent: Some(intent),
            new_branch_name,
            selected_integration_base_ref,
            selected_branch_to_work_on,
            ..ChangesetWorkflow::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a39";

    fn a_request<'a>() -> BranchIntentRequest<'a> {
        BranchIntentRequest::default()
    }

    #[test]
    fn an_omitted_intent_generates_a_branch_named_after_the_spawn_path_and_the_session() {
        // When
        let resolved = resolve_branch_workflow(
            SESSION_ID,
            &a_request(),
            BranchIntentPolicy::claude_cli(),
            None,
        )
        .expect("an omitted intent is the documented default");

        // Then
        assert_eq!(resolved.intent, BranchWorktreeIntent::NewBranchFromBase);
        assert_eq!(
            resolved.workflow.new_branch_name.as_deref(),
            Some("claude-cli/019d105b")
        );
    }

    #[test]
    fn a_new_branch_without_a_name_is_refused_where_the_policy_requires_one() {
        // When
        let error = resolve_branch_workflow(
            SESSION_ID,
            &BranchIntentRequest {
                branch_worktree_intent: "new_branch_from_base",
                ..a_request()
            },
            BranchIntentPolicy::workspace(),
            None,
        )
        .expect_err("a named intent must name its branch");

        // Then
        assert!(
            error.message().contains("new_branch_name"),
            "the refusal must name the missing field; got '{}'",
            error.message()
        );
    }

    #[test]
    fn a_new_branch_without_a_name_is_generated_where_the_policy_allows_it() {
        // When — cursor-cli has always accepted this shape
        let resolved = resolve_branch_workflow(
            SESSION_ID,
            &BranchIntentRequest {
                branch_worktree_intent: "new_branch_from_base",
                ..a_request()
            },
            BranchIntentPolicy::cursor_cli(),
            None,
        )
        .expect("cursor-cli generates a name rather than refusing");

        // Then
        assert_eq!(
            resolved.workflow.new_branch_name.as_deref(),
            Some("cursor-cli/019d105b")
        );
    }

    #[test]
    fn an_unrecognized_intent_is_refused_where_the_policy_refuses_it() {
        // When
        let error = resolve_branch_workflow(
            SESSION_ID,
            &BranchIntentRequest {
                branch_worktree_intent: "keep_current_branch",
                ..a_request()
            },
            BranchIntentPolicy::workspace(),
            None,
        )
        .expect_err("a workspace session refuses an intent it does not recognise");

        // Then — a split session's branch was chosen in the new-session form on another host, and
        // quietly substituting a generated one looks like the session that was asked for
        assert!(
            error.message().contains("keep_current_branch"),
            "the refusal must quote the offending value; got '{}'",
            error.message()
        );
    }

    #[test]
    fn the_project_default_branch_is_the_base_for_a_new_branch_with_no_explicit_base() {
        // When
        let resolved = resolve_branch_workflow(
            SESSION_ID,
            &a_request(),
            BranchIntentPolicy::claude_cli(),
            Some("origin/develop"),
        )
        .expect("resolve");

        // Then
        assert_eq!(
            resolved.workflow.selected_integration_base_ref.as_deref(),
            Some("origin/develop")
        );
    }

    #[test]
    fn the_project_default_branch_is_ignored_when_working_on_a_selected_branch() {
        // When
        let resolved = resolve_branch_workflow(
            SESSION_ID,
            &BranchIntentRequest {
                branch_worktree_intent: "work_on_selected_branch",
                selected_branch_to_work_on: "feature-x",
                ..a_request()
            },
            BranchIntentPolicy::claude_cli(),
            Some("origin/develop"),
        )
        .expect("resolve");

        // Then — the worktree joins an existing branch; a base ref would say what to cut it from
        assert_eq!(resolved.workflow.selected_integration_base_ref, None);
        assert_eq!(
            resolved.workflow.selected_branch_to_work_on.as_deref(),
            Some("feature-x")
        );
    }

    #[test]
    fn an_explicit_base_ref_wins_over_the_project_default() {
        // When
        let resolved = resolve_branch_workflow(
            SESSION_ID,
            &BranchIntentRequest {
                selected_integration_base_ref: "origin/release-2",
                ..a_request()
            },
            BranchIntentPolicy::claude_cli(),
            Some("origin/develop"),
        )
        .expect("resolve");

        // Then
        assert_eq!(
            resolved.workflow.selected_integration_base_ref.as_deref(),
            Some("origin/release-2")
        );
    }
}

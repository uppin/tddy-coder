use std::path::Path;

/// Refuse a `StartSessionRequest.pr_stack_base_session_id` that cannot seed a stack, *before*
/// anything spawns.
///
/// The pre-spawn position is the point: a refusal raised after the spawn is invisible to the
/// new-session form, which has already navigated away, so the operator would be left with an
/// orchestrator that looks seeded and is not. The seeding function refuses the same conditions again
/// as its own writer contract — the CLI flag is reachable without this RPC.
///
/// A blank id validates nothing, because nothing was asked for: an unseeded orchestrator is the
/// pre-existing behaviour. Otherwise the recipe must resolve to `"pr-stack"` (the legacy
/// `plan-pr-stack` / `orchestrate-pr-stack` aliases resolve to it and are accepted — refusing them
/// would make the recipe name a load-bearing string rather than a resolution), the named session must
/// pass [`tddy_workflow_recipes::pr_stack::check_stack_seed_base`] — the *same* rules, in the *same*
/// words, that the seeding writer enforces — and its repository must be the requesting project's.
///
/// **The repository check is this function's own**, because only the RPC knows which project was
/// asked for. Without it an operator can seed a stack with a branch from a different repository:
/// nothing refuses it, and the failure lands much later as a git error when the first descendant tries
/// to base off `origin/<branch>`, by which time the orchestrator exists and looks seeded. It compares
/// **canonicalized repository roots**, never project ids — a project id is registry-local and not
/// stable across hosts, while the repository root is the thing a stacked branch must actually share.
// `result_large_err`: the refusal is what the tonic gRPC surface reports to the new-session form, so
// `tonic::Status` is the error type — the same reason the adapter's streaming handlers allow it.
#[allow(clippy::result_large_err)]
pub fn validate_stack_seed_base_session(
    sessions_base: &Path,
    recipe: &str,
    base_session_id: &str,
    project_repo_root: &Path,
) -> Result<(), tonic::Status> {
    let base_session_id = base_session_id.trim();
    if base_session_id.is_empty() {
        return Ok(());
    }

    let is_pr_stack =
        tddy_workflow_recipes::recipe_resolve::resolve_workflow_recipe_from_cli_name(recipe.trim())
            .map(|r| r.name() == "pr-stack")
            .unwrap_or(false);
    if !is_pr_stack {
        return Err(tonic::Status::invalid_argument(format!(
            "pr_stack_base_session_id is only supported for the pr-stack recipe, but this session \
             requested recipe {recipe:?}"
        )));
    }

    // One rule, one wording: the refusal text lives in the recipes crate beside the writer, and only
    // the *code* it travels as is decided here — an id that names nothing is a bad argument, a session
    // whose state cannot seed is a failed precondition.
    let base =
        tddy_workflow_recipes::pr_stack::check_stack_seed_base(sessions_base, base_session_id)
            .map_err(|refusal| match refusal {
                tddy_workflow_recipes::pr_stack::StackSeedBaseRefusal::Unresolvable(reason) => {
                    tonic::Status::invalid_argument(reason)
                }
                tddy_workflow_recipes::pr_stack::StackSeedBaseRefusal::Unusable(reason) => {
                    tonic::Status::failed_precondition(reason)
                }
            })?;

    let base_repo = base.repo_path.as_deref().ok_or_else(|| {
        tonic::Status::failed_precondition(format!(
            "session '{base_session_id}' records no repository, so it cannot be confirmed to work in \
             this project's repository"
        ))
    })?;
    if !session_repo_is_in_project(Path::new(base_repo), project_repo_root).map_err(|reason| {
        tonic::Status::failed_precondition(format!(
            "session '{base_session_id}' could not be checked against this project's repository: \
             {reason}"
        ))
    })? {
        return Err(tonic::Status::failed_precondition(format!(
            "session '{base_session_id}' works in repository '{base_repo}', not this project's \
             '{}', so its branch cannot be stacked on here",
            project_repo_root.display()
        )));
    }
    Ok(())
}

/// Whether a session's recorded repository is the project's repository, or a worktree inside it.
///
/// The relation is "at or under", not equality, because `Changeset.repo_path` records the project's
/// main repo for a `tddy-coder` session but the session's **own worktree**
/// (`<repo>/.worktrees/<name>`) for a claude-cli / cursor-cli / workspace session. Both work in the
/// project's repository; only one of them spells its root.
///
/// Both sides are canonicalized: a project registered through a symlinked path and a session that
/// recorded the resolved one name the same repository, and a string comparison would call them
/// different. An unresolvable path is an `Err`, not a `false` — "could not tell" and "different
/// repository" are different answers, and only one of them may be reported as a mismatch.
fn session_repo_is_in_project(
    session_repo: &Path,
    project_repo_root: &Path,
) -> Result<bool, String> {
    let canonical = |path: &Path| -> Result<std::path::PathBuf, String> {
        path.canonicalize()
            .map_err(|e| format!("'{}' could not be resolved: {e}", path.display()))
    };
    Ok(canonical(session_repo)?.starts_with(canonical(project_repo_root)?))
}

//! Session worktrees created on a remote host over OpenSSH.

/// Same operations `tddy_core::worktree::setup_worktree_for_session` runs today, executed on `ssh_config_host` through
/// OpenSSH (`RemoteShell`): ensure the project checkout exists on the target (clone `git_url` there
/// if needed), then `git worktree add` under `.worktrees/`. Returns the absolute path on the target.
///
/// A BatchMode failure is the result — never a local worktree.
pub fn setup_worktree_for_session_over_ssh(
    ssh_config_host: &str,
    git_url: &str,
    session_id: &str,
) -> Result<String, String> {
    use crate::ssh_exec::{default_remote_repo_root, run_ssh_batch, shell_single_quote};

    let host = ssh_config_host.trim();
    if host.is_empty() {
        return Err("ssh_config_host is required for remote worktree setup".to_string());
    }
    let remote_repo = default_remote_repo_root(git_url);
    let worktree_path = format!("{}/.worktrees/{}", remote_repo, session_id);
    let quoted_repo = shell_single_quote(&remote_repo);
    let quoted_worktree = shell_single_quote(&worktree_path);
    let clone_url = git_url.trim();
    let clone_step = if clone_url.is_empty() {
        format!(
            "mkdir -p {quoted_repo} && if [ ! -d {quoted_repo}/.git ]; then \
             git init {quoted_repo} && git -C {quoted_repo} config user.email t@t.com && \
             git -C {quoted_repo} config user.name T && \
             git -C {quoted_repo} commit --allow-empty -m init; fi",
            quoted_repo = quoted_repo,
        )
    } else {
        let quoted_url = shell_single_quote(clone_url);
        format!(
            "if [ ! -d {quoted_repo}/.git ]; then \
             if [ -d {quoted_repo} ]; then \
               git init {quoted_repo}; \
             else \
               mkdir -p $(dirname {quoted_repo}) && git clone {quoted_url} {quoted_repo}; \
             fi; \
             git -C {quoted_repo} config user.email t@t.com; \
             git -C {quoted_repo} config user.name T; \
             git -C {quoted_repo} rev-parse HEAD >/dev/null 2>&1 || git -C {quoted_repo} commit --allow-empty -m init; \
             fi",
            quoted_repo = quoted_repo,
            quoted_url = quoted_url,
        )
    };

    let remote_cmd = format!(
        "set -e; {clone_step}; git -C {quoted_repo} fetch --all 2>/dev/null || true; \
         mkdir -p {quoted_repo}/.worktrees; \
         if git -C {quoted_repo} worktree list --porcelain | grep -q {quoted_worktree}; then \
           exit 0; \
         fi; \
         branch=tddy-{session_id}; \
         if git -C {quoted_repo} show-ref --verify --quiet refs/heads/$branch; then \
           git -C {quoted_repo} worktree add {quoted_worktree} $branch; \
         else \
           git -C {quoted_repo} worktree add -b $branch {quoted_worktree} HEAD; \
         fi",
        clone_step = clone_step,
        quoted_repo = quoted_repo,
        quoted_worktree = quoted_worktree,
        session_id = session_id,
    );

    run_ssh_batch(host, &remote_cmd)?;
    Ok(worktree_path)
}

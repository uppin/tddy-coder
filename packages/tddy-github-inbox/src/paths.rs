//! The REST paths the inbox reads — one place, so a scripted GitHub in tests answers the same paths.

use tddy_github::trunk_feed::RepoSlug;

fn repo_path(repo: &RepoSlug) -> String {
    format!("repos/{}/{}", repo.owner, repo.name)
}

/// The repository's activity feed.
pub fn events(repo: &RepoSlug) -> String {
    format!("{}/events?per_page=100", repo_path(repo))
}

/// Recently updated pull requests (the catch-up sweep).
pub fn updated_pulls(repo: &RepoSlug) -> String {
    format!(
        "{}/pulls?state=all&sort=updated&direction=desc&per_page=100",
        repo_path(repo)
    )
}

pub fn pull(repo: &RepoSlug, number: u64) -> String {
    format!("{}/pulls/{number}", repo_path(repo))
}

pub fn pulls_for_head(repo: &RepoSlug, branch: &str) -> String {
    format!(
        "{}/pulls?state=all&head={}:{branch}&per_page=1",
        repo_path(repo),
        repo.owner
    )
}

pub fn check_runs(repo: &RepoSlug, sha: &str) -> String {
    format!("{}/commits/{sha}/check-runs?per_page=100", repo_path(repo))
}

pub fn reviews(repo: &RepoSlug, number: u64) -> String {
    format!("{}/pulls/{number}/reviews?per_page=100", repo_path(repo))
}

pub fn review_comments(repo: &RepoSlug, number: u64) -> String {
    format!("{}/pulls/{number}/comments?per_page=100", repo_path(repo))
}

pub fn issue_comments(repo: &RepoSlug, number: u64) -> String {
    format!("{}/issues/{number}/comments?per_page=100", repo_path(repo))
}

pub fn pr_commits(repo: &RepoSlug, number: u64) -> String {
    format!("{}/pulls/{number}/commits?per_page=100", repo_path(repo))
}

pub fn branch_commits(repo: &RepoSlug, branch: &str) -> String {
    format!("{}/commits?sha={branch}&per_page=100", repo_path(repo))
}

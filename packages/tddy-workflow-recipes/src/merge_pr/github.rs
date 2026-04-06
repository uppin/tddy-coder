//! Optional GitHub REST merge for **merge-pr** (Green: token detection, PR lookup, `POST` merge).

/// Parameters for merging the open PR for the current branch (Green).
#[derive(Debug, Clone, Default)]
pub struct MergePrGithubParams {
    pub owner: &'static str,
    pub repo: &'static str,
    pub branch: &'static str,
}

/// RED skeleton: no HTTP yet.
pub fn merge_open_pr_for_branch(_params: MergePrGithubParams) -> Result<String, String> {
    let marker = r#"{"tddy":{"marker_id":"M004","scope":"merge_pr::github::merge_open_pr_for_branch","data":{}}}"#;
    eprintln!("{marker}");
    Err("merge-pr RED skeleton: GitHub merge not implemented".to_string())
}

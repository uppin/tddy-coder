//! GitHub REST merge for **merge-pr** (token detection, PR lookup, merge API).

use std::fs;
use std::process::Command;

use serde_json::Value;
use uuid::Uuid;

use crate::github_rest_common::{
    github_token_from_env, GITHUB_ACCEPT, GITHUB_API_VERSION, USER_AGENT_MERGE_PR,
};

/// Parameters for merging the open PR for the current branch.
#[derive(Debug, Clone, Default)]
pub struct MergePrGithubParams {
    pub owner: String,
    pub repo: String,
    pub branch: String,
}

fn temp_response_path(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}.json", Uuid::new_v4()))
}

/// Find open PR for `head={owner}:{branch}`, then merge via REST; returns merge commit SHA.
pub fn merge_open_pr_for_branch(params: MergePrGithubParams) -> Result<String, String> {
    let token = github_token_from_env().ok_or_else(|| {
        "merge-pr: GitHub credentials missing; set GITHUB_TOKEN or GH_TOKEN".to_string()
    })?;

    if params.owner.is_empty() || params.repo.is_empty() || params.branch.is_empty() {
        return Err("merge-pr: GitHub owner, repository, and branch must be non-empty".to_string());
    }

    let pulls_path = temp_response_path("tddy-gh-pulls");
    let pulls_url = format!(
        "https://api.github.com/repos/{}/{}/pulls",
        params.owner, params.repo
    );
    let head = format!("{}:{}", params.owner, params.branch);

    let pulls_http = curl_github_get_with_query(
        &pulls_url,
        &[("state", "open"), ("head", head.as_str())],
        &token,
        &pulls_path,
    )?;

    if !(200..300).contains(&pulls_http) {
        fs::remove_file(&pulls_path).ok();
        return Err(format!(
            "merge-pr: GitHub API list pulls failed with HTTP {pulls_http}"
        ));
    }

    let pulls_raw = fs::read_to_string(&pulls_path).map_err(|e| e.to_string())?;
    fs::remove_file(&pulls_path).ok();
    let pulls: Value = serde_json::from_str(&pulls_raw).map_err(|e| {
        format!("merge-pr: failed to parse GitHub pulls JSON: {e}; body={pulls_raw:?}")
    })?;

    let items = pulls
        .as_array()
        .ok_or_else(|| format!("merge-pr: unexpected GitHub pulls response: {pulls_raw}"))?;

    let pr = items.first().ok_or_else(|| {
        format!(
            "merge-pr: no open pull request for head {}:{}",
            params.owner, params.branch
        )
    })?;

    let number = pr
        .get("number")
        .and_then(|n| n.as_u64())
        .ok_or_else(|| format!("merge-pr: pull response missing number: {pr}"))?;

    let merge_path = temp_response_path("tddy-gh-merge");
    let merge_url = format!(
        "https://api.github.com/repos/{}/{}/pulls/{}/merge",
        params.owner, params.repo, number
    );

    let merge_body_path = temp_response_path("tddy-gh-merge-body");
    fs::write(&merge_body_path, br#"{"merge_method":"merge"}"#).map_err(|e| e.to_string())?;

    let merge_http = curl_github_put_json(&merge_url, &merge_body_path, &token, &merge_path)?;
    fs::remove_file(&merge_body_path).ok();

    if !(200..300).contains(&merge_http) {
        let merge_raw = fs::read_to_string(&merge_path).unwrap_or_default();
        fs::remove_file(&merge_path).ok();
        return Err(format!(
            "merge-pr: GitHub merge API failed with HTTP {merge_http}: {merge_raw}"
        ));
    }

    let merge_raw = fs::read_to_string(&merge_path).map_err(|e| e.to_string())?;
    fs::remove_file(&merge_path).ok();

    let merged: Value = serde_json::from_str(&merge_raw).map_err(|e| {
        format!("merge-pr: failed to parse merge response: {e}; body={merge_raw:?}")
    })?;

    merged
        .get("sha")
        .and_then(|s| s.as_str())
        .map(std::string::ToString::to_string)
        .ok_or_else(|| format!("merge-pr: merge response missing sha: {merge_raw}"))
}

fn curl_github_get_with_query(
    base_url: &str,
    query: &[(&str, &str)],
    token: &str,
    out_path: &std::path::Path,
) -> Result<u16, String> {
    let mut cmd = Command::new("curl");
    cmd.arg("-sS")
        .arg("-L")
        .arg("-o")
        .arg(out_path)
        .arg("-w")
        .arg("%{http_code}")
        .arg("-G")
        .arg(base_url);
    for (k, v) in query {
        cmd.arg("--data-urlencode").arg(format!("{k}={v}"));
    }
    cmd.arg("-H")
        .arg(format!("Authorization: Bearer {token}"))
        .arg("-H")
        .arg(format!("Accept: {GITHUB_ACCEPT}"))
        .arg("-H")
        .arg(format!("User-Agent: {USER_AGENT_MERGE_PR}"))
        .arg("-H")
        .arg(format!("X-GitHub-Api-Version: {GITHUB_API_VERSION}"));

    let out = cmd
        .output()
        .map_err(|e| format!("curl (GitHub GET): {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let code_str = String::from_utf8_lossy(&out.stdout);
    code_str
        .trim()
        .parse::<u16>()
        .map_err(|e| format!("curl: invalid HTTP status {code_str:?}: {e}"))
}

fn curl_github_put_json(
    url: &str,
    json_body_path: &std::path::Path,
    token: &str,
    out_path: &std::path::Path,
) -> Result<u16, String> {
    let out = Command::new("curl")
        .arg("-sS")
        .arg("-L")
        .arg("-o")
        .arg(out_path)
        .arg("-w")
        .arg("%{http_code}")
        .arg("-X")
        .arg("PUT")
        .arg("-H")
        .arg(format!("Authorization: Bearer {token}"))
        .arg("-H")
        .arg(format!("Accept: {GITHUB_ACCEPT}"))
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("-H")
        .arg(format!("User-Agent: {USER_AGENT_MERGE_PR}"))
        .arg("-H")
        .arg(format!("X-GitHub-Api-Version: {GITHUB_API_VERSION}"))
        .arg("--data-binary")
        .arg(format!("@{}", json_body_path.display()))
        .arg(url)
        .output()
        .map_err(|e| format!("curl (GitHub PUT): {e}"))?;

    if !out.status.success() {
        return Err(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let code_str = String::from_utf8_lossy(&out.stdout);
    code_str
        .trim()
        .parse::<u16>()
        .map_err(|e| format!("curl: invalid HTTP status {code_str:?}: {e}"))
}

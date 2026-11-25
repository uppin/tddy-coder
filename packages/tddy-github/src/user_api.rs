//! The account a token belongs to — `GET /user` — as the commit path derives it.
//!
//! When a vault record predates the provider identifiers, the commit path asks the provider
//! itself who the record's token belongs to, then stamps the answer back onto the record. The
//! fetch is sync, over the same curl transport as every other REST call in this crate, and takes
//! its `api_base` as a value — [`GITHUB_API_BASE`](crate::github_rest_common::GITHUB_API_BASE) in
//! production, a local listener where a test stands in for GitHub — never from the environment.

use serde_json::Value;

use crate::github_rest_common::curl_github_get_json_absolute_path;
use crate::provider::GitHubUser;

/// Parse a `GET /user` answer (`{"id": …, "login": …}`).
///
/// The id and login are required: they are what the commit identity is derived from, and an
/// answer without them identifies nobody. `avatar_url` and `name` are best-effort — GitHub sends
/// `name: null` when the person never set one.
pub fn parse_authenticated_user(body: &str) -> Result<GitHubUser, String> {
    let user: Value = serde_json::from_str(body).map_err(|e| format!("not JSON: {e}"))?;
    let id = user
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "the /user answer has no numeric id".to_string())?;
    let login = user
        .get("login")
        .and_then(Value::as_str)
        .filter(|login| !login.is_empty())
        .ok_or_else(|| "the /user answer has no login".to_string())?;
    Ok(GitHubUser {
        id,
        login: login.to_string(),
        avatar_url: user
            .get("avatar_url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        name: user
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

/// Ask `api_base` who `token` belongs to.
///
/// A blank token is refused before curl runs; any other failure carries the transport's reason
/// as a plain sentence — no `WorkflowError` wrapper — for the derivation refusal to quote.
pub fn fetch_authenticated_user(api_base: &str, token: &str) -> Result<GitHubUser, String> {
    let body = curl_github_get_json_absolute_path(api_base, "user", &[], token)
        .map_err(transport_reason)?;
    parse_authenticated_user(&body)
}

/// The curl failure's own sentence. The transport wraps every one as `WriteFailed`, whose
/// `Display` prefixes "artifact write failed" — the wording of a write, not of this read.
fn transport_reason(error: tddy_core::WorkflowError) -> String {
    match error {
        tddy_core::WorkflowError::WriteFailed(reason) => reason,
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn a_user_answer_parses_to_its_id_and_login() {
        // Given GitHub's answer for Ada
        let body = r#"{"login":"ada","id":101,"avatar_url":"https://example.invalid/ada.png","name":"Ada"}"#;

        // When the commit path parses it
        let user = parse_authenticated_user(body).expect("Ada is Ada");

        // Then it is her id and login
        assert_eq!(
            user,
            GitHubUser {
                id: 101,
                login: "ada".to_string(),
                avatar_url: "https://example.invalid/ada.png".to_string(),
                name: "Ada".to_string(),
            }
        );
    }

    #[test]
    fn an_unset_name_parses_to_empty_rather_than_failing() {
        // Given GitHub's answer for a person who never set a name
        let body = r#"{"login":"ada","id":101,"name":null}"#;

        // When the commit path parses it
        let user = parse_authenticated_user(body).expect("null is not a refusal");

        // Then the name is empty and the identity stands
        assert_eq!(user.name, String::new());
        assert_eq!(user.login, "ada");
    }

    #[test]
    fn an_answer_without_an_id_identifies_nobody() {
        // Given an answer with no id
        let body = r#"{"login":"ada"}"#;

        // When the commit path parses it
        let failure = parse_authenticated_user(body).expect_err("there is no id");

        // Then the refusal says so
        assert!(failure.contains("id"), "got: {failure}");
    }

    #[test]
    fn an_answer_without_a_login_identifies_nobody() {
        // Given an answer with an empty login
        let body = r#"{"login":"","id":101}"#;

        // When the commit path parses it
        let failure = parse_authenticated_user(body).expect_err("there is no login");

        // Then the refusal says so
        assert!(failure.contains("login"), "got: {failure}");
    }

    #[test]
    fn a_body_that_is_not_json_is_not_a_user() {
        // Given a body that is not JSON at all
        let body = "gateway timeout";

        // When the commit path parses it
        let failure = parse_authenticated_user(body).expect_err("that is no user");

        // Then the refusal says so
        assert!(failure.contains("JSON"), "got: {failure}");
    }
}

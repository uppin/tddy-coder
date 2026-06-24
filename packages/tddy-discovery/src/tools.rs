//! READ/GLOB/GREP tool executor for the FastContext Discovery loop.
//!
//! `ToolExecutor` has two modes:
//! - `Local`: executes against the local filesystem (std::fs, glob, regex).
//! - `Remote`: POSTs to `{daemon_url}/connection.ConnectionService/ExecuteTool` with the
//!   `ExecuteToolRequest` envelope built from a `RemoteToolEnv`, mapping the `result_json`
//!   shapes: Read→`{"content":...}`, Grep→`{"matches":[...]}`, Glob→`{"paths":[...]}`.
//!   `is_error: true` responses are surfaced as errors (no silent fallback).
//!
//! The executor mode is selected from `InvokeRequest.remote`: Some → Remote, None → Local.

use regex::Regex;
use tddy_core::backend::{InvokeRequest, RemoteToolEnv};

/// Result of executing a single tool call.
#[derive(Debug)]
pub struct ToolOutput {
    /// The raw result JSON parsed into a structured value.
    pub value: serde_json::Value,
}

/// Read/Glob/Grep executor with local and remote modes.
pub enum ToolExecutor {
    /// Execute against the local filesystem.
    Local,
    /// Execute via the relay → `ExecuteTool` RPC against a remote worktree.
    Remote(RemoteToolEnv),
}

/// Response body from `ExecuteTool` RPC.
#[derive(serde::Deserialize)]
struct ExecuteToolResponse {
    result_json: Option<String>,
    is_error: bool,
    error_message: Option<String>,
}

impl ToolExecutor {
    /// Construct an executor from an `InvokeRequest`: Remote when `remote` is Some, Local otherwise.
    pub fn from_invoke_request(req: &InvokeRequest) -> Self {
        match req.remote.clone() {
            Some(env) => ToolExecutor::Remote(env),
            None => ToolExecutor::Local,
        }
    }

    /// Execute a READ tool call: return file contents as `{"content": "..."}`.
    pub async fn read(
        &self,
        path: &str,
        _offset: Option<u64>,
        _limit: Option<u64>,
    ) -> Result<ToolOutput, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            ToolExecutor::Local => {
                let content =
                    std::fs::read_to_string(path).map_err(|e| format!("READ {path}: {e}"))?;
                Ok(ToolOutput {
                    value: serde_json::json!({ "content": content }),
                })
            }
            ToolExecutor::Remote(env) => {
                let args = serde_json::json!({ "path": path });
                self.execute_remote(env, "Read", args).await
            }
        }
    }

    /// Execute a GLOB tool call: return matching paths as `{"paths": [...]}`.
    pub async fn glob(
        &self,
        pattern: &str,
    ) -> Result<ToolOutput, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            ToolExecutor::Local => {
                let mut paths: Vec<String> = Vec::new();
                for entry in glob::glob(pattern)
                    .map_err(|e| format!("GLOB pattern error: {e}"))?
                    .flatten()
                {
                    if let Some(s) = entry.to_str() {
                        paths.push(s.to_string());
                    }
                }
                Ok(ToolOutput {
                    value: serde_json::json!({ "paths": paths }),
                })
            }
            ToolExecutor::Remote(env) => {
                let args = serde_json::json!({ "pattern": pattern });
                self.execute_remote(env, "Glob", args).await
            }
        }
    }

    /// Execute a GREP tool call: return matches as `{"matches": [...]}`.
    pub async fn grep(
        &self,
        pattern: &str,
        path: Option<&str>,
    ) -> Result<ToolOutput, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            ToolExecutor::Local => {
                let re = Regex::new(pattern)
                    .map_err(|e| format!("GREP invalid regex {pattern:?}: {e}"))?;
                let mut matches: Vec<serde_json::Value> = Vec::new();

                let search_path = path.unwrap_or(".");
                let metadata = std::fs::metadata(search_path);
                let is_file = metadata.as_ref().map(|m| m.is_file()).unwrap_or(false);

                if is_file {
                    Self::grep_file(&re, search_path, &mut matches);
                } else {
                    // Walk directory tree
                    Self::grep_dir(&re, search_path, &mut matches);
                }

                Ok(ToolOutput {
                    value: serde_json::json!({ "matches": matches }),
                })
            }
            ToolExecutor::Remote(env) => {
                let mut args = serde_json::json!({ "pattern": pattern });
                if let Some(p) = path {
                    args["path"] = serde_json::Value::String(p.to_string());
                }
                self.execute_remote(env, "Grep", args).await
            }
        }
    }

    /// POST an ExecuteTool request to the remote daemon.
    async fn execute_remote(
        &self,
        env: &RemoteToolEnv,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<ToolOutput, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!(
            "{}/connection.ConnectionService/ExecuteTool",
            env.daemon_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "session_id": env.session_id,
            "session_token": env.session_token,
            "tool_name": tool_name,
            "args_json": args.to_string(),
        });
        let client = reqwest::Client::new();
        let resp = client.post(&url).json(&body).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("ExecuteTool HTTP {status}: {text}").into());
        }
        let rpc: ExecuteToolResponse = resp.json().await?;
        if rpc.is_error {
            let msg = rpc
                .error_message
                .unwrap_or_else(|| "ExecuteTool returned is_error=true".to_string());
            return Err(msg.into());
        }
        let result_str = rpc
            .result_json
            .ok_or("ExecuteTool: result_json is null but is_error=false")?;
        let value: serde_json::Value = serde_json::from_str(&result_str)
            .map_err(|e| format!("ExecuteTool: invalid result_json: {e}"))?;
        Ok(ToolOutput { value })
    }

    fn grep_file(re: &Regex, path: &str, matches: &mut Vec<serde_json::Value>) {
        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };
        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                matches.push(serde_json::json!({
                    "type": "match",
                    "data": {
                        "path": { "text": path },
                        "line_number": i + 1,
                        "lines": { "text": line }
                    }
                }));
            }
        }
    }

    fn grep_dir(re: &Regex, dir: &str, matches: &mut Vec<serde_json::Value>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(path_str) = path.to_str() else {
                continue;
            };
            if path.is_file() {
                Self::grep_file(re, path_str, matches);
            } else if path.is_dir() {
                Self::grep_dir(re, path_str, matches);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests: ToolExecutor local and remote modes.
    //!
    //! Feature: docs/ft/coder/discovery-agent.md (Phase C criteria 9–11)
    //! Changeset: docs/dev/1-WIP/2026-06-24-changeset-fastcontext-discovery.md

    use std::io::Write;

    use tddy_core::backend::RemoteToolEnv;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::ToolExecutor;

    fn make_remote_tool_env(daemon_url: &str) -> RemoteToolEnv {
        RemoteToolEnv {
            daemon_url: daemon_url.to_string(),
            session_id: "sess-test-123".to_string(),
            session_token: "tok-abc".to_string(),
            daemon_instance_id: Some("relay-local".to_string()),
            livekit_url: None,
            livekit_room: None,
            server_identity: None,
        }
    }

    // ─── Local mode ────────────────────────────────────────────────────────────

    /// Local READ returns the file's contents.
    #[tokio::test]
    async fn local_read_tool_returns_file_content() {
        // Given — a temp file with known contents
        let tmp = std::env::temp_dir().join("tddy-discovery-read-test.txt");
        let mut f = std::fs::File::create(&tmp).unwrap();
        writeln!(f, "line 1").unwrap();
        writeln!(f, "line 2").unwrap();
        let path_str = tmp.to_str().unwrap();

        // When
        let output = ToolExecutor::Local
            .read(path_str, None, None)
            .await
            .expect("local READ must succeed for an existing file");

        // Then — content field is present and non-empty
        let content = output.value["content"]
            .as_str()
            .expect("READ result must have a string 'content' field");
        assert!(
            content.contains("line 1"),
            "READ content must include file contents; got: {content:?}"
        );
    }

    /// Local GLOB returns paths matching the pattern.
    #[tokio::test]
    async fn local_glob_tool_returns_matching_paths() {
        // Given — a temp dir with two .txt files
        let dir = std::env::temp_dir().join("tddy-discovery-glob-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "").unwrap();
        std::fs::write(dir.join("b.txt"), "").unwrap();
        std::fs::write(dir.join("c.rs"), "").unwrap();
        let pattern = format!("{}/*.txt", dir.display());

        // When
        let output = ToolExecutor::Local
            .glob(&pattern)
            .await
            .expect("local GLOB must succeed");

        // Then — paths contains the two .txt files but not .rs
        let paths = output.value["paths"]
            .as_array()
            .expect("GLOB result must have a 'paths' array");
        let path_strs: Vec<&str> = paths.iter().filter_map(|v| v.as_str()).collect();
        assert_eq!(
            path_strs.len(),
            2,
            "GLOB must match exactly 2 .txt files; got {path_strs:?}"
        );
        assert!(
            path_strs.iter().any(|p| p.ends_with("a.txt")),
            "GLOB must include a.txt; got {path_strs:?}"
        );
        assert!(
            path_strs.iter().any(|p| p.ends_with("b.txt")),
            "GLOB must include b.txt; got {path_strs:?}"
        );
    }

    /// Local GREP returns lines matching the pattern.
    #[tokio::test]
    async fn local_grep_tool_returns_matching_lines() {
        // Given — a temp file with a known pattern
        let tmp = std::env::temp_dir().join("tddy-discovery-grep-test.txt");
        std::fs::write(&tmp, "fn authenticate() {\n    let x = 1;\n}\n").unwrap();
        let path_str = tmp.to_str().unwrap();

        // When
        let output = ToolExecutor::Local
            .grep("fn authenticate", Some(path_str))
            .await
            .expect("local GREP must succeed");

        // Then — matches array is present and non-empty
        let matches = output.value["matches"]
            .as_array()
            .expect("GREP result must have a 'matches' array");
        assert!(
            !matches.is_empty(),
            "GREP must find at least one match for 'fn authenticate'"
        );
    }

    // ─── Remote mode ───────────────────────────────────────────────────────────

    /// Remote READ POSTs ExecuteToolRequest with the correct envelope fields and maps the
    /// `result_json` `{"content": "..."}` shape to a `ToolOutput`.
    #[tokio::test]
    async fn remote_executor_posts_execute_tool_and_maps_read_content() {
        // Given — a mock ExecuteTool endpoint
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/connection.ConnectionService/ExecuteTool"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result_json": "{\"content\": \"file contents here\"}",
                "is_error": false,
                "error_message": null,
                "job_id": null,
                "job_running": false
            })))
            .mount(&server)
            .await;

        let env = make_remote_tool_env(&server.uri());
        let executor = ToolExecutor::Remote(env);

        // When
        let output = executor
            .read("src/main.rs", None, None)
            .await
            .expect("remote READ must succeed against mock ExecuteTool");

        // Then — content is mapped from result_json
        let content = output.value["content"]
            .as_str()
            .expect("remote READ must map result_json content field");
        assert_eq!(content, "file contents here");

        // Verify the mock was hit (request envelope correctness checked by mock matching)
        let received = server.received_requests().await.unwrap();
        assert_eq!(
            received.len(),
            1,
            "exactly one ExecuteTool request must be sent"
        );
        let body: serde_json::Value =
            serde_json::from_slice(&received[0].body).expect("request body must be valid JSON");
        assert_eq!(
            body["tool_name"].as_str(),
            Some("Read"),
            "tool_name must be 'Read'"
        );
        assert_eq!(
            body["session_id"].as_str(),
            Some("sess-test-123"),
            "session_id must come from RemoteToolEnv"
        );
        assert_eq!(
            body["session_token"].as_str(),
            Some("tok-abc"),
            "session_token must come from RemoteToolEnv"
        );
    }

    /// Remote GREP response is correctly mapped from the ripgrep-JSON `matches` shape.
    #[tokio::test]
    async fn remote_executor_maps_grep_matches_shape() {
        // Given
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/connection.ConnectionService/ExecuteTool"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result_json": "{\"matches\": [{\"type\": \"match\", \"data\": {\"path\": {\"text\": \"src/lib.rs\"}, \"line_number\": 10}}]}",
                "is_error": false,
                "error_message": null,
                "job_id": null,
                "job_running": false
            })))
            .mount(&server)
            .await;

        let executor = ToolExecutor::Remote(make_remote_tool_env(&server.uri()));

        // When
        let output = executor
            .grep("fn authenticate", Some("src/"))
            .await
            .expect("remote GREP must succeed");

        // Then
        let matches = output.value["matches"]
            .as_array()
            .expect("remote GREP result must have a 'matches' array");
        assert_eq!(matches.len(), 1, "one match must be returned");
    }

    /// Remote GLOB response is correctly mapped from the `paths` shape.
    #[tokio::test]
    async fn remote_executor_maps_glob_paths_shape() {
        // Given
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/connection.ConnectionService/ExecuteTool"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result_json": "{\"paths\": [\"src/lib.rs\", \"src/main.rs\"]}",
                "is_error": false,
                "error_message": null,
                "job_id": null,
                "job_running": false
            })))
            .mount(&server)
            .await;

        let executor = ToolExecutor::Remote(make_remote_tool_env(&server.uri()));

        // When
        let output = executor
            .glob("src/**/*.rs")
            .await
            .expect("remote GLOB must succeed");

        // Then
        let paths = output.value["paths"]
            .as_array()
            .expect("remote GLOB result must have a 'paths' array");
        assert_eq!(paths.len(), 2);
    }

    /// When the ExecuteTool response has `is_error: true`, the executor surfaces it as an error
    /// (no silent fallback, no panic).
    #[tokio::test]
    async fn remote_executor_surfaces_is_error_responses() {
        // Given — mock returns is_error: true
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/connection.ConnectionService/ExecuteTool"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "result_json": null,
                "is_error": true,
                "error_message": "path escapes worktree root",
                "job_id": null,
                "job_running": false
            })))
            .mount(&server)
            .await;

        let executor = ToolExecutor::Remote(make_remote_tool_env(&server.uri()));

        // When
        let result = executor.read("../../etc/passwd", None, None).await;

        // Then — error is surfaced (not silently swallowed)
        assert!(
            result.is_err(),
            "remote executor must return Err when is_error is true; got Ok"
        );
        let msg = result.unwrap_err().to_string();
        assert!(!msg.is_empty(), "error message must be non-empty");
    }

    /// When `InvokeRequest.remote` is `Some(RemoteToolEnv)`, the executor is `Remote`.
    /// Verified by constructing the executor from an `InvokeRequest` and checking its variant.
    #[tokio::test]
    async fn executor_selects_remote_mode_when_remote_tool_env_present() {
        use tddy_core::backend::InvokeRequest;

        // Given
        let env = make_remote_tool_env("http://localhost:9999");
        let req = InvokeRequest {
            prompt: "find auth".to_string(),
            remote: Some(env),
            ..InvokeRequest::default()
        };

        let executor = ToolExecutor::from_invoke_request(&req);

        // Then — variant is Remote
        assert!(
            matches!(executor, ToolExecutor::Remote(_)),
            "ToolExecutor must be Remote when InvokeRequest.remote is Some"
        );
    }

    /// When `InvokeRequest.remote` is `None`, the executor is `Local`.
    #[tokio::test]
    async fn executor_selects_local_mode_when_remote_tool_env_absent() {
        use tddy_core::backend::InvokeRequest;

        // Given
        let req = InvokeRequest {
            prompt: "find auth".to_string(),
            remote: None,
            ..InvokeRequest::default()
        };

        let executor = ToolExecutor::from_invoke_request(&req);

        // Then — variant is Local
        assert!(
            matches!(executor, ToolExecutor::Local),
            "ToolExecutor must be Local when InvokeRequest.remote is None"
        );
    }
}

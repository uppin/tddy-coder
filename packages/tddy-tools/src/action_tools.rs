//! Session-action MCP tools for a session whose `Shell` is replaced by a subagent (see
//! docs/ft/coder/no-bash-mode.md).
//!
//! With `Shell` hard-disabled, the sandboxed agent runs commands only through declarative
//! session actions (`tddy_core::session_actions`): it *requests* a new action in natural
//! language (`request_action`), the Shell-replacing author subagent (the def in
//! `TDDY_SUBAGENTS_JSON` with `replaces: [Shell]`, e.g. a local gemma via Ollama) writes the
//! YAML manifest, and — once it validates — the action is auto-established under
//! `<session_dir>/actions/` and invocable via `invoke_action`.
//!
//! Trust model: this module runs in the jail, so its validation is a cheap retry loop for the
//! author, never the authority. The manifest is re-validated and written host-side by the
//! `EstablishAction` relay handler (`tddy-sandbox-app::host_actions`), and `list_actions`/
//! `invoke_action` are host round-trips too — the session dir only exists on the host.

use tddy_core::session_actions::{
    parse_action_manifest_yaml, validate_authored_manifest, ActionManifest,
};
use tddy_discovery::subagent::SubagentRegistry;

use crate::server::{
    schema_object, shell_replacing_author, subagent_config_from_env, subagent_error_json,
    subagent_route, subagents_from_env, PermissionServer,
};

/// Bounded correction loop with the author model: initial attempt + this many retries carrying
/// the previous validation error back as the next turn.
const MAX_AUTHOR_ATTEMPTS: usize = 3;

/// Upper bound on the authored manifest text — a manifest is a small YAML file; anything larger
/// is a runaway generation, not an action.
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// The authoring instructions sent to the action-author subagent. The author has READ/GLOB/GREP
/// over the (managed) codebase to look up real commands and paths before answering.
fn author_prompt(description: &str, suggested_id: Option<&str>) -> String {
    let id_hint = suggested_id
        .map(|id| format!("Use `{id}` as the manifest `id` if it fits.\n"))
        .unwrap_or_default();
    format!(
        "Write a session-action YAML manifest for the following request, and reply with ONLY \
         the YAML (inside <final_answer>...</final_answer> or a fenced code block).\n\
         \n\
         Request: {description}\n\
         {id_hint}\
         \n\
         Manifest schema (unknown keys are rejected):\n\
         - version: 1                      (required, literal)\n\
         - id: <kebab-case-name>           (required; letters, digits, `-`, `_` only)\n\
         - summary: <one line>             (required)\n\
         - architecture: native            (required, literal)\n\
         - command: [<program>, <arg>, …]  (required; a literal argv vector — the program and \
         each argument as its own list element. NO shell string, NO `sh -c`, NO placeholders or \
         templating.)\n\
         - input_schema: <JSON Schema object>   (optional; only if the caller must pass data)\n\
         - result_kind: test_summary            (optional; only for cargo-style test runs whose \
         output ends in a `test result:` totals line)\n\
         \n\
         Keep the command as narrow as the request allows (e.g. `[cargo, test, -p, some-pkg]` \
         rather than a broad wrapper). You may READ/GLOB/GREP the codebase first to find the \
         right program, package, or script name."
    )
}

/// Pull the manifest YAML out of the author's answer: a fenced code block when present
/// (`\u{60}\u{60}\u{60}yaml` or bare fences), otherwise the trimmed answer itself. The
/// `<final_answer>` envelope is already stripped by the subagent session loop.
fn extract_manifest_yaml(answer: &str) -> String {
    let trimmed = answer.trim();
    if let Some(fence_start) = trimmed.find("```") {
        let after_fence = &trimmed[fence_start + 3..];
        // Skip an info string like `yaml` up to the first newline.
        let body_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
        let body = &after_fence[body_start..];
        let body_end = body.find("```").unwrap_or(body.len());
        return body[..body_end].trim().to_string();
    }
    trimmed.to_string()
}

/// In-jail pre-validation: parse, bound, and sanity-check an authored manifest before spending a
/// host round-trip. The host's `EstablishAction` handler repeats all of this authoritatively.
fn prevalidate_manifest_yaml(yaml: &str) -> Result<ActionManifest, String> {
    if yaml.trim().is_empty() {
        return Err("the reply contained no YAML".to_string());
    }
    if yaml.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "manifest is {} bytes; the limit is {MAX_MANIFEST_BYTES}",
            yaml.len()
        ));
    }
    let manifest = parse_action_manifest_yaml(yaml).map_err(|e| e.to_string())?;
    validate_authored_manifest(&manifest).map_err(|e| e.to_string())?;
    Ok(manifest)
}

/// `request_action`: describe a needed command; the configured author subagent writes the
/// manifest; on successful validation it is established host-side and immediately invocable.
async fn request_action_tool(args: serde_json::Value) -> String {
    let Some(description) = args
        .get("description")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    else {
        return subagent_error_json("missing required field: description");
    };
    let suggested_id = args.get("suggested_id").and_then(|v| v.as_str());

    let defs = subagents_from_env();
    let Some(author) = shell_replacing_author(&defs) else {
        return subagent_error_json(
            "no action author configured: session actions require a subagent whose `replaces` \
             covers Shell (add `replaces: [Shell]` to its def in the sandbox config)",
        );
    };
    let registry = SubagentRegistry::from_defs(defs);
    let mut session = match registry.create(&author, subagent_config_from_env()) {
        Ok(session) => session,
        Err(e) => return subagent_error_json(format!("action author '{author}': {e}")),
    };

    let mut prompt = author_prompt(description, suggested_id);
    let mut last_error = String::new();
    for _attempt in 0..MAX_AUTHOR_ATTEMPTS {
        let outcome = match session.prompt(&prompt).await {
            Ok(outcome) => outcome,
            Err(e) => return subagent_error_json(format!("action author '{author}': {e}")),
        };
        let answer = outcome
            .content
            .iter()
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let yaml = extract_manifest_yaml(&answer);
        match prevalidate_manifest_yaml(&yaml) {
            Ok(_) => {
                // Auto-establish: the host re-validates authoritatively and writes
                // `<session_dir>/actions/<id>.yaml`; its summary JSON is the tool result.
                return crate::session_tool_client::dispatch_session_tool(
                    "EstablishAction",
                    serde_json::json!({ "yaml": yaml }),
                )
                .await;
            }
            Err(e) => {
                last_error = e;
                prompt = format!(
                    "That manifest was rejected: {last_error}\n\
                     Reply with ONLY the corrected YAML manifest."
                );
            }
        }
    }
    subagent_error_json(format!(
        "action author '{author}' produced no valid manifest in {MAX_AUTHOR_ATTEMPTS} attempts; \
         last error: {last_error}"
    ))
}

/// `list_actions`: host round-trip to `tddy_core::session_actions::list_action_summaries`.
async fn list_actions_tool(args: serde_json::Value) -> String {
    crate::session_tool_client::dispatch_session_tool("ListActions", args).await
}

/// `invoke_action`: host round-trip to the blocking `invoke_action_core` (synchronous in v1 —
/// the relay already carries long-running Shell calls over the same path).
async fn invoke_action_tool(args: serde_json::Value) -> String {
    if args.get("action").and_then(|v| v.as_str()).is_none() {
        return subagent_error_json("missing required field: action");
    }
    crate::session_tool_client::dispatch_session_tool("InvokeAction", args).await
}

/// Build the `ToolRouter` for the three session-action tools. Merged into
/// `PermissionServer::new()`'s router only when a configured def replaces `Shell`
/// (`shell_replacing_author`).
pub(crate) fn action_tool_router() -> rmcp::handler::server::router::tool::ToolRouter<PermissionServer>
{
    use rmcp::handler::server::router::tool::ToolRouter;

    let mut router = ToolRouter::new();

    let request_tool = rmcp::model::Tool::new(
        "request_action",
        "Request a new session action: describe the command you need in natural language; the \
         configured action-author agent writes a bounded manifest for it. Once established, run \
         it with invoke_action. Returns {id, summary, path, has_input_schema}.",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["description"],
            "properties": {
                "description": {
                    "type": "string",
                    "description": "What the action should do, e.g. 'run the tddy-core test suite'."
                },
                "suggested_id": {
                    "type": "string",
                    "description": "Optional kebab-case id for the new action."
                }
            }
        })),
    );
    router.add_route(subagent_route(request_tool, |args| {
        Box::pin(request_action_tool(args))
    }));

    let list_tool = rmcp::model::Tool::new(
        "list_actions",
        "List the session actions available to invoke_action. \
         Returns {actions: [{id, summary, has_input_schema, has_output_schema}]}.",
        schema_object(serde_json::json!({
            "type": "object",
            "properties": {}
        })),
    );
    router.add_route(subagent_route(list_tool, |args| {
        Box::pin(list_actions_tool(args))
    }));

    let invoke_tool = rmcp::model::Tool::new(
        "invoke_action",
        "Invoke an established session action by id, blocking until it exits. \
         Returns {exit_code, stdout, stderr} (plus summary for result_kind: test_summary).",
        schema_object(serde_json::json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {"type": "string", "description": "Action id (see list_actions)."},
                "data": {
                    "type": "object",
                    "description": "JSON arguments validated against the action's input_schema."
                }
            }
        })),
    );
    router.add_route(subagent_route(invoke_tool, |args| {
        Box::pin(invoke_action_tool(args))
    }));

    router
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_MANIFEST: &str = "\
version: 1
id: run-core-tests
summary: Run the tddy-core test suite
architecture: native
command: [cargo, test, -p, tddy-core]
";

    // ─── extract_manifest_yaml ──────────────────────────────────────────────────

    /// A bare YAML answer passes through trimmed.
    #[test]
    fn extract_returns_a_bare_yaml_answer_trimmed() {
        assert_eq!(
            extract_manifest_yaml(&format!("\n{VALID_MANIFEST}\n")),
            VALID_MANIFEST.trim()
        );
    }

    /// A fenced ```yaml block is unwrapped — models often fence even when told not to.
    #[test]
    fn extract_unwraps_a_fenced_yaml_block() {
        let answer = format!("Here is the manifest:\n```yaml\n{VALID_MANIFEST}```\n");
        assert_eq!(extract_manifest_yaml(&answer), VALID_MANIFEST.trim());
    }

    /// A bare fence (no info string) is unwrapped too.
    #[test]
    fn extract_unwraps_a_bare_fenced_block() {
        let answer = format!("```\n{VALID_MANIFEST}```");
        assert_eq!(extract_manifest_yaml(&answer), VALID_MANIFEST.trim());
    }

    // ─── prevalidate_manifest_yaml ──────────────────────────────────────────────

    /// The canonical valid manifest passes pre-validation.
    #[test]
    fn a_valid_manifest_prevalidates() {
        let manifest = prevalidate_manifest_yaml(VALID_MANIFEST)
            .expect("the canonical manifest must validate");
        assert_eq!(manifest.id, "run-core-tests");
        assert_eq!(manifest.command[0], "cargo");
    }

    /// Unknown YAML keys are rejected (deny_unknown_fields), matching the host parser.
    #[test]
    fn a_manifest_with_unknown_keys_is_rejected() {
        let yaml = format!("{VALID_MANIFEST}bogus_key: 1\n");
        prevalidate_manifest_yaml(&yaml).expect_err("unknown keys must be rejected");
    }

    /// An empty command vector is rejected before any host round-trip.
    #[test]
    fn an_empty_command_is_rejected() {
        let yaml = "\
version: 1
id: nothing
summary: does nothing
architecture: native
command: []
";
        let err = prevalidate_manifest_yaml(yaml).expect_err("empty argv must be rejected");
        assert!(err.contains("command"), "got: {err}");
    }

    /// A path-traversal id (`../x`, `a/b`) is rejected — the id becomes a filename under the
    /// session actions dir on the host.
    #[test]
    fn a_path_traversal_id_is_rejected() {
        for bad_id in ["../escape", "a/b", "a.b"] {
            let yaml = format!(
                "version: 1\nid: {bad_id}\nsummary: s\narchitecture: native\ncommand: [echo]\n"
            );
            let err = prevalidate_manifest_yaml(&yaml)
                .expect_err(&format!("id {bad_id:?} must be rejected"));
            assert!(err.contains("id"), "got: {err}");
        }
    }

    /// A non-compiling `input_schema` is caught in the retry loop, not shipped to the host.
    #[test]
    fn a_broken_input_schema_is_rejected() {
        let yaml = "\
version: 1
id: with-schema
summary: s
architecture: native
command: [echo]
input_schema:
  type: 42
";
        let err = prevalidate_manifest_yaml(yaml).expect_err("a broken schema must be rejected");
        assert!(err.contains("input_schema"), "got: {err}");
    }

    /// An oversized manifest is a runaway generation, rejected by the byte cap.
    #[test]
    fn an_oversized_manifest_is_rejected() {
        let yaml = format!("{VALID_MANIFEST}# {}\n", "x".repeat(MAX_MANIFEST_BYTES));
        let err = prevalidate_manifest_yaml(&yaml).expect_err("oversized YAML must be rejected");
        assert!(err.contains("bytes"), "got: {err}");
    }
}

//! Authoring a session-action manifest: what to ask an agent for, how to get the YAML back out of
//! its answer, and how far to pre-validate it before spending a host round-trip.
//!
//! Moved here from `tddy-tools`' `action_tools` by `#unbundle` node 5. The MCP *advertisement* of
//! `request_action` / `list_actions` / `invoke_action` stayed there — the shape of an MCP tool
//! belongs to the crate that speaks MCP — but nothing below names `rmcp`: it is the manifest rules
//! this module's own crate already owns, sitting one call away from
//! [`parse_action_manifest_yaml`](super::parse_action_manifest_yaml) and
//! [`validate_authored_manifest`](super::validate_authored_manifest), which it calls.

use super::{parse_action_manifest_yaml, validate_authored_manifest, ActionManifest};

/// Bounded correction loop with the author model: initial attempt + this many retries carrying
/// the previous validation error back as the next turn.
pub const MAX_AUTHOR_ATTEMPTS: usize = 3;

/// Upper bound on the authored manifest text — a manifest is a small YAML file; anything larger
/// is a runaway generation, not an action.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// The authoring instructions sent to the action-author subagent. The author has READ/GLOB/GREP
/// over the (managed) codebase to look up real commands and paths before answering.
pub fn author_prompt(description: &str, suggested_id: Option<&str>) -> String {
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
pub fn extract_manifest_yaml(answer: &str) -> String {
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
pub fn prevalidate_manifest_yaml(yaml: &str) -> Result<ActionManifest, String> {
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

    /// The prompt names the agent's own suggestion when the call carried one, so the author is
    /// not left to invent an id the caller already chose.
    #[test]
    fn the_author_prompt_carries_a_suggested_id() {
        // When
        let prompt = author_prompt("run the tddy-core test suite", Some("run-core-tests"));

        // Then
        assert!(prompt.contains("run the tddy-core test suite"), "{prompt}");
        assert!(
            prompt.contains("Use `run-core-tests` as the manifest `id`"),
            "{prompt}"
        );
    }
}

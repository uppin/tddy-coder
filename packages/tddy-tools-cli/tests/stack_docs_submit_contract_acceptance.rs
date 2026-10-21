//! Acceptance: the `write-stack-docs` prompt and `tddy-tools submit` describe the same payload.
//!
//! The goal shipped telling the agent to submit **YAML** under a `submit` tool key, while
//! `tddy-tools submit` parses JSON and routes on `--goal`. An agent that followed the prompt was
//! refused before the hook ever ran, and no test saw it: every one drove the hook directly with a
//! hand-written string. These tests take the payload out of the prompt itself and push it through
//! the real CLI, so prompt, schema and hook cannot drift apart again — the same class of defect
//! [#411](../../../docs/dev/changesets/) closed for `write-stack-plan`.

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tddy_workflow_recipes::plan_pr_stack::write_stack_docs_system_prompt;

// ── Builders ────────────────────────────────────────────────────────────────────────────────

/// The example payload the `write-stack-docs` system prompt shows the agent, lifted from the
/// prompt's own fenced `json` block. Reading it out of the prompt rather than restating it here is
/// the whole point: a prompt that starts describing a shape `submit` refuses fails this suite.
fn the_payload_the_prompt_shows() -> String {
    let prompt = write_stack_docs_system_prompt("pr-stack");
    let opening = "```json\n";
    let body_start = prompt
        .find(opening)
        .expect("the write-stack-docs prompt must show a fenced json example")
        + opening.len();
    let body = &prompt[body_start..];
    let body_end = body
        .find("```")
        .expect("the prompt's fenced json example must be closed");
    body[..body_end].trim().to_string()
}

/// A complete pass over a one-node stack: the PRD, and the changeset carrying all four sections.
fn a_document_pair_for_one_planned_node() -> String {
    r##"{"goal":"write-stack-docs","version":1,"docs":[{"node_id":"token-store","prd":"# token-store — Auth token store\n","changeset":"# Changeset: token-store\n\n## Responsibility\nOwns the token store.\n\n## Boundaries\nNot request authentication.\n\n## Dependencies\nNone — a root node.\n\n## Draft PR contract\n`trait TokenStore` plus its failing tests.\n"}]}"##
        .to_string()
}

/// The shape the prompt asked for before this fix: a YAML document, which `submit` cannot parse.
fn a_stack_docs_payload_written_as_yaml() -> String {
    "version: 1\n\
     docs:\n  \
       - node_id: token-store\n    \
         prd: |\n      \
           # token-store — Auth token store\n    \
         changeset: |\n      \
           ## Responsibility\n      \
           Owns the token store.\n"
        .to_string()
}

/// A document pair whose `changeset` is missing — the boundaries document is the one that stops two
/// children building the same abstraction, so a pair without it is not a pair.
fn a_docs_payload_missing_its_changeset() -> String {
    r##"{"goal":"write-stack-docs","version":1,"docs":[{"node_id":"token-store","prd":"# token-store — Auth token store\n"}]}"##
        .to_string()
}

/// A `submit` invocation made the way the prompt instructs: `--goal write-stack-docs` with the body
/// on stdin. `TDDY_SOCKET` is removed so the call takes the no-socket path and reports the goal it
/// resolved rather than relaying to a live session's listener.
struct SubmitCall {
    goal_flag: String,
    payload: String,
}

fn a_stack_docs_submit() -> SubmitCall {
    SubmitCall {
        goal_flag: "write-stack-docs".to_string(),
        payload: a_document_pair_for_one_planned_node(),
    }
}

impl SubmitCall {
    fn with_payload(mut self, payload: String) -> Self {
        self.payload = payload;
        self
    }

    fn run(self) -> SubmitOutcome {
        let mut cmd = cargo_bin_cmd!("tddy-tools");
        cmd.env_remove("TDDY_SOCKET");
        cmd.args(["submit", "--goal", &self.goal_flag, "--data-stdin"]);
        cmd.write_stdin(self.payload.clone());
        let output = cmd.output().expect("run tddy-tools submit");
        SubmitOutcome {
            code: output.status.code().expect("submit exited via a signal"),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

struct SubmitOutcome {
    code: i32,
    stdout: String,
    stderr: String,
}

// ── Assertions ──────────────────────────────────────────────────────────────────────────────

impl SubmitOutcome {
    fn assert_acknowledged_goal(&self, expected: &str) -> &Self {
        assert_eq!(
            self.code, 0,
            "expected submit to succeed, got exit {} (stdout={}, stderr={})",
            self.code, self.stdout, self.stderr
        );
        let reported: Value = serde_json::from_str(self.stdout.trim())
            .expect("submit stdout must be one JSON object");
        assert_eq!(
            reported["goal"].as_str(),
            Some(expected),
            "submit must acknowledge the goal it routed on; got {}",
            self.stdout
        );
        assert_eq!(
            reported["status"].as_str(),
            Some("ok"),
            "expected an ok status; got {}",
            self.stdout
        );
        self
    }

    fn assert_refused_the_body(&self) -> &Self {
        assert_eq!(
            self.code, 1,
            "expected exit 1 (unparseable body), got {} (stdout={}, stderr={})",
            self.code, self.stdout, self.stderr
        );
        assert!(
            self.stdout.contains("\"status\":\"error\""),
            "an unparseable body must be reported as an error status on stdout; got {}",
            self.stdout
        );
        self
    }

    fn assert_validation_error(&self) -> &Self {
        assert_eq!(
            self.code, 3,
            "expected exit 3 (validation error), got {} (stdout={}, stderr={})",
            self.code, self.stdout, self.stderr
        );
        assert!(
            self.stdout.contains("\"status\":\"error\"") && self.stdout.contains("\"errors\""),
            "validation failure must surface the schema errors; got {}",
            self.stdout
        );
        self
    }

    fn assert_reports(&self, fragment: &str) -> &Self {
        let combined = format!("{}{}", self.stdout, self.stderr);
        assert!(
            combined.contains(fragment),
            "expected submit to report '{fragment}'; got stdout={} stderr={}",
            self.stdout,
            self.stderr
        );
        self
    }
}

// ── The prompt's own example is the contract ────────────────────────────────────────────────

/// The test the original goal lacked: the agent copies this payload out of its system prompt and
/// runs exactly this command. If `submit` refuses it, the goal cannot be completed by any agent
/// that follows its instructions.
#[test]
fn the_payload_the_prompt_shows_is_accepted_by_submit_under_the_write_stack_docs_goal() {
    // Given — the fenced example from the write-stack-docs system prompt
    let call = a_stack_docs_submit().with_payload(the_payload_the_prompt_shows());

    // When — submitted the way the prompt instructs: --goal plus a heredoc on stdin
    let outcome = call.run();

    // Then
    outcome.assert_acknowledged_goal("write-stack-docs");
}

/// The defect itself: the prompt asked for YAML, which `submit` parses as JSON and refuses. A
/// prompt that reverts to a YAML example makes the test above fail on this same error.
#[test]
fn submit_refuses_a_stack_docs_payload_written_as_yaml() {
    // Given
    let call = a_stack_docs_submit().with_payload(a_stack_docs_payload_written_as_yaml());

    // When
    let outcome = call.run();

    // Then
    outcome.assert_refused_the_body().assert_reports("JSON");
}

// ── `write-stack-docs` carries a registered schema ──────────────────────────────────────────

/// `get-schema <goal>` is the discovery path the prompt points the agent at. It has to answer for
/// `write-stack-docs` or the prompt sends it somewhere empty.
#[test]
fn get_schema_publishes_the_stack_docs_contract_for_write_stack_docs() {
    // Given
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd.args(["get-schema", "write-stack-docs"]);

    // When
    let output = cmd.output().expect("run tddy-tools get-schema");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    // Then
    assert_eq!(
        output.status.code(),
        Some(0),
        "get-schema write-stack-docs must succeed; stdout={stdout} stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let schema: Value =
        serde_json::from_str(stdout.trim()).expect("get-schema stdout must be JSON Schema");
    assert_eq!(
        schema["$id"].as_str(),
        Some("urn:tddy:goal/write-stack-docs"),
        "schema must carry the goal's URN id; got {stdout}"
    );
    let required: Vec<&str> = schema["required"]
        .as_array()
        .expect("schema must declare required fields")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(
        required,
        vec!["goal", "version", "docs"],
        "schema must pin the fields the write-stack-docs hook parses"
    );
}

/// Without a schema this payload relayed as ok and failed later, inside the hook, on a serde error
/// naming no node. Registered, it fails here with the missing field named.
#[test]
fn submit_rejects_a_document_pair_that_omits_its_changeset() {
    // Given — a PRD with no boundaries document beside it
    let call = a_stack_docs_submit().with_payload(a_docs_payload_missing_its_changeset());

    // When
    let outcome = call.run();

    // Then
    outcome
        .assert_validation_error()
        .assert_reports("changeset");
}

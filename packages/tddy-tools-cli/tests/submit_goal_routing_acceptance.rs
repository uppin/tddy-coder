//! Acceptance tests for `tddy-tools submit` goal routing and the `write-stack-plan` contract.
//!
//! Both behaviours come from one incident (session `01a04d4b-84f8-7fc0-b020-19ae73981175`): a
//! pr-stack agent ran `tddy-tools submit --goal write-stack-plan` twice with a `{key, value}`
//! payload and got `{"goal":"unknown","status":"ok"}` both times. Two defects compounded — the
//! relayed goal is read only from the JSON body so `--goal` was silently discarded, and
//! `write-stack-plan` has no registered schema so the wrong shape was never validated. Between
//! them the agent got two green lights for two no-ops and spent the next ninety seconds reading
//! `cli.rs` to work out why nothing had happened.

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;

/// The shape the `write-stack-plan` hook actually parses into `StackPlanOutput`.
fn a_valid_stack_plan_payload() -> String {
    r#"{"goal":"write-stack-plan","version":1,"prs":[{"node_id":"token-store","title":"Auth token store","description":"Store tokens in the keyring, with tests","branch_suggestion":"feature/auth/token-store","parents":[]}]}"#
        .to_string()
}

/// The shape the incident agent inferred from "using the `submit` tool with key `stack-plan`" —
/// a plan the workflow cannot parse, which `submit` nonetheless acknowledged as ok.
fn a_stack_plan_payload_shaped_as_key_and_value() -> String {
    r#"{"key":"stack-plan","value":"version: 1\nprs: []\n"}"#.to_string()
}

/// A `submit` invocation. `TDDY_SOCKET` is removed so the call takes the no-socket path and
/// reports the goal it resolved rather than relaying it to a live session's listener.
struct SubmitCall {
    goal_flag: Option<String>,
    payload: String,
}

fn a_submit() -> SubmitCall {
    SubmitCall {
        goal_flag: Some("write-stack-plan".to_string()),
        payload: a_valid_stack_plan_payload(),
    }
}

impl SubmitCall {
    fn with_goal_flag(mut self, goal: &str) -> Self {
        self.goal_flag = Some(goal.to_string());
        self
    }

    fn without_goal_flag(mut self) -> Self {
        self.goal_flag = None;
        self
    }

    fn with_payload(mut self, payload: String) -> Self {
        self.payload = payload;
        self
    }

    fn run(self) -> SubmitOutcome {
        let mut cmd = cargo_bin_cmd!("tddy-tools");
        cmd.env_remove("TDDY_SOCKET");
        cmd.arg("submit");
        if let Some(ref goal) = self.goal_flag {
            cmd.args(["--goal", goal]);
        }
        cmd.args(["--data", &self.payload]);
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

    fn assert_usage_error(&self) -> &Self {
        assert_eq!(
            self.code, 2,
            "expected exit 2 (usage error), got {} (stdout={}, stderr={})",
            self.code, self.stdout, self.stderr
        );
        assert!(
            self.stdout.contains("\"status\":\"error\""),
            "usage error must be reported as an error status on stdout; got {}",
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

// ── `--goal` is the routing goal, not merely a schema selector ──────────────────────────────

/// Ten recipe goals (`assess`, `orchestrate`, `grill`, `interview`, …) have no registered schema,
/// so nothing forces their payload to carry a `goal` field. For those, `--goal` is the only signal
/// of which task the submission belongs to — discarding it routes the output to nobody.
#[test]
fn submit_routes_on_the_goal_flag_when_the_payload_carries_no_goal_field() {
    // Given — an unregistered goal whose payload names no goal of its own
    let call = a_submit()
        .with_goal_flag("assess")
        .with_payload(r#"{"summary":"3 nodes ready to merge, 1 needs a repoint"}"#.to_string());

    // When
    let outcome = call.run();

    // Then
    outcome.assert_acknowledged_goal("assess");
}

/// A payload that names one goal submitted under another is an agent that has lost track of which
/// task it is answering. Silently picking either one is how a submission lands on the wrong task.
#[test]
fn submit_rejects_a_payload_whose_goal_field_contradicts_the_goal_flag() {
    // Given — a valid plan payload submitted under the demo goal
    let call = a_submit()
        .with_goal_flag("demo")
        .with_payload(r##"{"goal":"plan","prd":"# PRD\n\n## Summary\nFeature X"}"##.to_string());

    // When
    let outcome = call.run();

    // Then — the disagreement is named, both sides quoted
    outcome
        .assert_usage_error()
        .assert_reports("demo")
        .assert_reports("plan");
}

// ── An unresolvable goal must fail loudly, never succeed as "unknown" ───────────────────────

/// The incident's first two submissions. With no goal in the flag or the body there is no task to
/// deliver to, so `status: ok` is a lie that costs the agent its next several turns.
#[test]
fn submit_refuses_a_payload_that_names_no_goal_in_the_flag_or_the_body() {
    // Given — the incident payload, submitted with no --goal
    let call = a_submit()
        .without_goal_flag()
        .with_payload(a_stack_plan_payload_shaped_as_key_and_value());

    // When
    let outcome = call.run();

    // Then
    outcome.assert_usage_error();
}

/// The remedy has to travel with the refusal: an agent that reached this error omitted `--goal`,
/// and telling it so is the difference between one corrected retry and a source-reading detour.
#[test]
fn submit_names_the_goal_flag_as_the_remedy_when_no_goal_resolves() {
    // Given — the incident payload, submitted with no --goal
    let call = a_submit()
        .without_goal_flag()
        .with_payload(a_stack_plan_payload_shaped_as_key_and_value());

    // When
    let outcome = call.run();

    // Then
    outcome.assert_reports("--goal");
}

// ── `write-stack-plan` carries a registered schema ──────────────────────────────────────────

/// `get-schema <goal>` is the discovery path the pr-stack prompt points agents at. It has to
/// answer for `write-stack-plan` or the prompt sends them somewhere empty.
#[test]
fn get_schema_publishes_the_stack_plan_contract_for_write_stack_plan() {
    // Given
    let mut cmd = cargo_bin_cmd!("tddy-tools");
    cmd.env_remove("TDDY_SOCKET");
    cmd.args(["get-schema", "write-stack-plan"]);

    // When
    let output = cmd.output().expect("run tddy-tools get-schema");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    // Then
    assert_eq!(
        output.status.code(),
        Some(0),
        "get-schema write-stack-plan must succeed; stdout={stdout} stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let schema: Value =
        serde_json::from_str(stdout.trim()).expect("get-schema stdout must be JSON Schema");
    assert_eq!(
        schema["$id"].as_str(),
        Some("urn:tddy:goal/write-stack-plan"),
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
        vec!["goal", "version", "prs"],
        "schema must pin the fields the write-stack-plan hook parses"
    );
}

/// The regression test for the incident: this exact payload was accepted as ok, leaving the
/// workflow with nothing to parse. Registering the schema turns it into a validation failure the
/// agent can act on in one turn.
#[test]
fn submit_rejects_a_stack_plan_shaped_as_key_and_value() {
    // Given — the incident payload, submitted under its intended goal
    let call = a_submit()
        .with_goal_flag("write-stack-plan")
        .with_payload(a_stack_plan_payload_shaped_as_key_and_value());

    // When
    let outcome = call.run();

    // Then
    outcome.assert_validation_error();
}

/// The near-miss the wholly-wrong `{key, value}` payload does not cover: a plan of the right
/// shape whose PR entry omits `node_id`. Every `parents` reference and every stack node is keyed
/// on it, so a plan without one cannot be turned into a DAG — the schema has to say so rather than
/// leaving `after_write_stack_plan` to fail later on a serde error.
#[test]
fn submit_rejects_a_stack_plan_whose_pr_entry_omits_its_node_id() {
    // Given — a well-formed plan missing the one field the DAG is keyed on
    let call = a_submit().with_goal_flag("write-stack-plan").with_payload(
        r#"{"goal":"write-stack-plan","version":1,"prs":[{"title":"Auth token store","description":"Store tokens in the keyring, with tests","branch_suggestion":"feature/auth/token-store","parents":[]}]}"#
            .to_string(),
    );

    // When
    let outcome = call.run();

    // Then
    outcome.assert_validation_error().assert_reports("node_id");
}

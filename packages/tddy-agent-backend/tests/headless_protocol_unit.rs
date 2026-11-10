//! Unit: the pieces the generic headless workflow backend is configured with (`#agent-tools 15/19`)
//! — goal hints to params, a route's model, templated env values, each adapter's failure detail and
//! transcript, the backend's name, and the turn files and session pointer a turn leaves behind.
//!
//! Changeset: `docs/dev/changesets/2026-11-05-agent-tools-headless-driver.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_agent_backend::adapters::{
    render_env, AdapterRegistry, EnvTemplate, GoalParams, HeadlessInvocation, HeadlessMcpServer,
    HeadlessProtocol, HeadlessRenderer, HeadlessTurn, RouteModel, CLAUDE_STREAM_JSON, CODEX_JSONL,
    CURSOR_STREAM, OZ_NDJSON,
};
use tddy_agent_backend::provider_route::{RouteSupport, RoutedAgent};
use tddy_agent_backend::session_pointer::{
    find_session_pointer, SessionPointer, SESSION_POINTER_FILE,
};
use tddy_agent_backend::stream::StreamResult;
use tddy_agent_backend::turn_files::TurnFiles;
use tddy_agent_backend::{CodingBackend, SpecHeadlessBackend};
use tddy_session_store::error::BackendError;
use tddy_workflow::{GoalHints, PermissionHint};

fn params(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// A goal's hints: `permission`, in plan mode or not.
fn a_goal(permission: PermissionHint, plan_mode: bool) -> GoalHints {
    GoalHints {
        display_name: "Goal".to_string(),
        permission,
        allowed_tools: Vec::new(),
        default_model: None,
        agent_output: false,
        agent_cli_plan_mode: plan_mode,
        claude_nonzero_exit_ok_if_structured_response: false,
    }
}

/// `claude.yaml`'s `goal_params`.
fn claudes_goal_params() -> GoalParams {
    GoalParams {
        read_only_plan: params(&[("permission_mode", "plan")]),
        accept_edits: params(&[("permission_mode", "acceptEdits")]),
        ..GoalParams::default()
    }
}

/// `cursor.yaml`'s `goal_params`.
fn cursors_goal_params() -> GoalParams {
    GoalParams {
        always: params(&[("force", "1")]),
        plan_mode: params(&[("plan", "1")]),
        ..GoalParams::default()
    }
}

/// `codex.yaml`'s `goal_params`.
fn codexs_goal_params() -> GoalParams {
    GoalParams {
        read_only_plan: params(&[("sandbox_mode", "read-only")]),
        ..GoalParams::default()
    }
}

// --- Goal hints → params ------------------------------------------------------------------------

#[rstest]
#[case::claude_plans_read_only(claudes_goal_params(), PermissionHint::ReadOnly, true, &[("permission_mode", "plan")])]
#[case::claude_reads_without_plan_mode(claudes_goal_params(), PermissionHint::ReadOnly, false, &[])]
#[case::claude_accepts_edits(claudes_goal_params(), PermissionHint::AcceptEdits, false, &[("permission_mode", "acceptEdits")])]
#[case::cursor_plans(cursors_goal_params(), PermissionHint::ReadOnly, true, &[("force", "1"), ("plan", "1")])]
#[case::cursor_always_forces(cursors_goal_params(), PermissionHint::AcceptEdits, false, &[("force", "1")])]
#[case::codex_plans_read_only(codexs_goal_params(), PermissionHint::ReadOnly, true, &[("sandbox_mode", "read-only")])]
#[case::codex_writes_otherwise(codexs_goal_params(), PermissionHint::AcceptEdits, true, &[])]
fn a_goals_hints_bind_the_params_its_agents_spec_declares_for_them(
    #[case] goal_params: GoalParams,
    #[case] permission: PermissionHint,
    #[case] plan_mode: bool,
    #[case] expected: &[(&str, &str)],
) {
    // Given
    let goal = a_goal(permission, plan_mode);

    // When
    let bound = goal_params.bind(&goal);

    // Then
    assert_eq!(bound, params(expected));
}

#[test]
fn a_later_goal_param_map_overrides_an_earlier_ones_name() {
    // Given
    let goal_params = GoalParams {
        always: params(&[("mode", "default")]),
        accept_edits: params(&[("mode", "edits")]),
        ..GoalParams::default()
    };

    // When
    let bound = goal_params.bind(&a_goal(PermissionHint::AcceptEdits, false));

    // Then
    assert_eq!(bound, params(&[("mode", "edits")]));
}

// --- A route's model ----------------------------------------------------------------------------

#[rstest]
#[case::kept_unrouted(RouteModel::Kept, &[], Some("opus"))]
#[case::kept_routed(RouteModel::Kept, &["-m", "muse"], Some("opus"))]
#[case::replaced_routed(RouteModel::Replaced, &["-c", "model_provider=\"x\"", "-m", "muse"], None)]
#[case::replaced_unrouted(RouteModel::Replaced, &[], Some("opus"))]
#[case::replaced_when_route_names_it(RouteModel::ReplacedIfRouteHas {
            flag: "--model".to_string(),
        }, &["--model", "muse"], None)]
#[case::kept_when_route_does_not_name_it(RouteModel::ReplacedIfRouteHas {
            flag: "--model".to_string(),
        }, &["--settings", "x"], Some("opus"))]
fn a_route_replaces_the_requests_model_only_where_the_spec_says(
    #[case] route_model: RouteModel,
    #[case] route_args: &[&str],
    #[case] expected: Option<&str>,
) {
    // When
    let model = route_model.model_for(Some("opus".to_string()), &strings(route_args));

    // Then
    assert_eq!(model, expected.map(str::to_string));
}

// --- Templated env values -----------------------------------------------------------------------

#[test]
fn an_env_value_naming_the_session_dir_and_self_exe_is_rendered_with_both() {
    // Given
    let env = vec![
        ("BROWSER".to_string(), "{self_exe}".to_string()),
        (
            "CODEX_OUT".to_string(),
            "{session_dir}/codex_oauth_authorize.url".to_string(),
        ),
        ("PLAIN".to_string(), "literal".to_string()),
    ];
    let template = EnvTemplate {
        session_dir: Some(PathBuf::from("/var/tddy/sessions/s1")),
        self_exe: PathBuf::from("/usr/bin/tddy-coder"),
    };

    // When
    let rendered = render_env(&env, &template);

    // Then
    assert_eq!(
        rendered.map_err(|e| e.to_string()),
        Ok(vec![
            ("BROWSER".to_string(), "/usr/bin/tddy-coder".to_string()),
            (
                "CODEX_OUT".to_string(),
                "/var/tddy/sessions/s1/codex_oauth_authorize.url".to_string()
            ),
            ("PLAIN".to_string(), "literal".to_string()),
        ])
    );
}

#[test]
fn an_env_value_naming_the_session_dir_of_a_request_with_none_is_refused_naming_the_variable() {
    // Given
    let env = vec![("CODEX_OUT".to_string(), "{session_dir}/x".to_string())];
    let template = EnvTemplate {
        session_dir: None,
        self_exe: PathBuf::from("/usr/bin/tddy-coder"),
    };

    // When
    let rendered = render_env(&env, &template);

    // Then
    assert_eq!(
        rendered.map_err(|e| e.to_string()),
        Err(BackendError::InvocationFailed(
            "headless env CODEX_OUT names {session_dir}, and the request has no session directory"
                .to_string()
        )
        .to_string())
    );
}

// --- Each adapter's failure detail and transcript -----------------------------------------------

fn a_stream_with_errors(errors: &[&str]) -> StreamResult {
    StreamResult {
        result_text: String::new(),
        session_id: String::new(),
        questions: Vec::new(),
        raw_lines: Vec::new(),
        stream_errors: strings(errors),
    }
}

#[rstest]
#[case::claude_joins_its_stream_errors(CLAUDE_STREAM_JSON, &["No conversation found", "rate limited"], "noise\n", "No conversation found; rate limited")]
#[case::claude_falls_back_to_its_stderr(CLAUDE_STREAM_JSON, &[], "  claude: auth expired \n", "claude: auth expired")]
#[case::cursor_reports_its_stderr(CURSOR_STREAM, &["ignored"], " cursor: no such chat \n", "cursor: no such chat")]
#[case::oz_reports_its_stderr(OZ_NDJSON, &[], "oz: profile not found\n", "oz: profile not found")]
#[case::codex_reports_its_last_stream_error(CODEX_JSONL, &["first", "model not found"], "tracing noise\n", "model not found")]
fn each_adapter_names_the_detail_a_failed_turn_reports(
    #[case] adapter: &str,
    #[case] stream_errors: &[&str],
    #[case] stderr: &str,
    #[case] expected: &str,
) {
    // Given
    let adapter = AdapterRegistry::builtin()
        .get(adapter)
        .expect("the adapter is registered");

    // When
    let detail = adapter.failure_detail(&a_stream_with_errors(stream_errors), stderr);

    // Then
    assert_eq!(detail, expected);
}

#[test]
fn a_codex_sign_in_failure_carries_the_sign_in_remediation_after_its_error() {
    // Given
    let adapter = AdapterRegistry::builtin()
        .get(CODEX_JSONL)
        .expect("codex-jsonl is registered");
    let stream = a_stream_with_errors(&[
        "unexpected status 401 Unauthorized: Missing bearer or basic authentication",
    ]);

    // When
    let detail = adapter.failure_detail(&stream, "");

    // Then
    let (error, remediation) = detail
        .split_once("\n\n")
        .expect("the remediation follows the error after a blank line");
    assert_eq!(
        (error, remediation.lines().next()),
        (
            "unexpected status 401 Unauthorized: Missing bearer or basic authentication",
            Some("Fix: not signed in to OpenAI for Codex. `codex exec --json` does not print a login URL or start browser OAuth — only API errors.")
        )
    );
}

#[test]
fn every_builtin_stream_adapter_makes_a_conversation_transcript() {
    // Given
    let registry = AdapterRegistry::builtin();

    // When
    let with_transcript: Vec<(String, bool)> = registry
        .names()
        .map(|name| {
            let adapter = registry.get(name).expect("registered");
            (name.to_string(), adapter.transcript(None).is_some())
        })
        .collect();

    // Then
    assert_eq!(
        with_transcript,
        vec![
            (CLAUDE_STREAM_JSON.to_string(), true),
            (CODEX_JSONL.to_string(), true),
            (CURSOR_STREAM.to_string(), true),
            (OZ_NDJSON.to_string(), true),
        ]
    );
}

// --- The backend's name -------------------------------------------------------------------------

#[derive(Debug)]
struct ARendererFor(&'static str);

impl HeadlessRenderer for ARendererFor {
    fn agent_id(&self) -> &str {
        self.0
    }

    fn tool_server(&self) -> Option<HeadlessMcpServer> {
        None
    }

    fn render(&self, _turn: &HeadlessTurn) -> Result<HeadlessInvocation, BackendError> {
        Err(BackendError::InvocationFailed(
            "not rendered here".to_string(),
        ))
    }
}

#[test]
fn the_generic_backend_is_named_by_its_specs_id() {
    // Given
    let backend = SpecHeadlessBackend::new(
        Arc::new(ARendererFor("lark")),
        HeadlessProtocol::default(),
        RoutedAgent {
            id: "lark".to_string(),
            product: "Lark Code".to_string(),
            support: RouteSupport::default(),
            key_env: None,
            config_env: None,
        },
    );

    // When
    let name = backend.name().to_string();

    // Then
    assert_eq!(name, "lark");
}

// --- Turn files and the session pointer ---------------------------------------------------------

/// A git checkout to run turns in, and its `info/exclude`.
struct ACheckout {
    dir: tempfile::TempDir,
}

fn a_checkout() -> ACheckout {
    let dir = tempfile::tempdir().expect("checkout");
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("git runs");
    assert!(status.success(), "git init");
    ACheckout { dir }
}

impl ACheckout {
    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn excluded(&self) -> Vec<String> {
        std::fs::read_to_string(self.path().join(".git/info/exclude"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .map(str::to_string)
            .collect()
    }
}

fn a_pointer(checkout: &Path) -> SessionPointer {
    SessionPointer {
        socket: Some(PathBuf::from("/run/tddy/s1.sock")),
        repo_dir: checkout.to_path_buf(),
        session_dir: Some(PathBuf::from("/var/tddy/sessions/s1")),
        remote: BTreeMap::new(),
        notify: BTreeMap::new(),
        owner: None,
    }
}

#[test]
fn a_turns_files_are_git_excluded_while_they_exist_and_removed_when_the_turn_ends() {
    // Given
    let checkout = a_checkout();
    let mcp_config = checkout.path().join(".cursor/mcp.json");
    let files = vec![(mcp_config.clone(), b"{}".to_vec())];

    // When
    let written = TurnFiles::write(&files, checkout.path()).expect("the turn's files are written");
    let while_running = (mcp_config.exists(), checkout.excluded());
    drop(written);

    // Then
    assert_eq!(
        (while_running, mcp_config.exists()),
        ((true, vec!["/.cursor/mcp.json".to_string()]), false)
    );
}

#[test]
fn a_turns_session_pointer_is_found_from_a_directory_below_it_and_removed_with_the_turn() {
    // Given
    let checkout = a_checkout();
    let below = checkout.path().join("packages/app/src");
    std::fs::create_dir_all(&below).expect("a nested directory");
    let mut files = TurnFiles::write(&[], checkout.path()).expect("no files");
    files
        .write_session_pointer(checkout.path(), &a_pointer(checkout.path()))
        .expect("the pointer is written");

    // When
    let found = find_session_pointer(&below).expect("the pointer reads");
    let excluded = checkout.excluded();
    drop(files);
    let after_the_turn = find_session_pointer(&below).expect("nothing to read");

    // Then
    assert_eq!(
        (found, excluded, after_the_turn),
        (
            Some((
                checkout.path().join(SESSION_POINTER_FILE),
                a_pointer(checkout.path())
            )),
            vec![format!("/{SESSION_POINTER_FILE}")],
            None
        )
    );
}

#[test]
fn a_session_pointer_that_does_not_parse_is_an_error_naming_the_file() {
    // Given
    let checkout = a_checkout();
    let pointer = checkout.path().join(SESSION_POINTER_FILE);
    std::fs::create_dir_all(pointer.parent().expect("parent")).expect(".tddy");
    std::fs::write(&pointer, "not json").expect("a broken pointer");

    // When
    let found = find_session_pointer(checkout.path());

    // Then
    assert_eq!(found.map_err(|e| e.path), Err(pointer));
}

#[test]
fn turn_files_in_a_directory_that_is_not_a_git_checkout_are_refused() {
    // Given
    let not_a_checkout = tempfile::tempdir().expect("plain dir");
    let files = vec![(
        not_a_checkout.path().join(".cursor/mcp.json"),
        b"{}".to_vec(),
    )];

    // When
    let written = TurnFiles::write(&files, not_a_checkout.path());

    // Then
    assert_eq!(
        (
            written.is_err(),
            not_a_checkout.path().join(".cursor/mcp.json").exists()
        ),
        (true, false)
    );
}

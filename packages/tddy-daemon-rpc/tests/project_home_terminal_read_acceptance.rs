//! Acceptance: a project-home session reads what another session's TUI shows right now —
//! the visible text of its main terminal and the cursor — on its own host or another
//! (`#home-inbox` 2/8).
//!
//! A session's TUI is stood in for by a shell script that draws a known screen with escape
//! sequences and then blocks on stdin, started as the session's main terminal in the daemon's own
//! terminal manager, so the tool reads the same screen model a browser attaching would be shown.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-home-inbox-terminal-read.md

mod support;

use std::sync::Arc;
use std::time::Duration;

use support::{
    a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, OTHER_PROJECT, THIS_PROJECT,
};
use tddy_rpc::{Code, Request};
use tddy_service::proto::exec_tools::{
    ExecToolService, ExecuteToolRequest, ExecuteToolResponse, ListExecToolsRequest,
};
use tddy_session_lifecycle::claude_cli_session::PtyHandle;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

/// The grid every PTY the daemon spawns starts at (`tddy_pty::DEFAULT_TERM_{COLS,ROWS}`).
const GRID_COLS: u64 = 220;
const GRID_ROWS: u64 = 24;
/// How long the stand-in TUI is given to draw its screen.
const DRAW_TIMEOUT: Duration = Duration::from_secs(15);
/// A well-formed session id that no host knows.
const UNKNOWN_SESSION: &str = "019d105b-ac0f-78d3-9a89-409731145aff";

struct World {
    a: Host,
    b: Host,
    home: String,
    on_a: String,
    on_b: String,
    foreign_on_a: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    let foreign_on_a = a_workspace_in(&a.daemon, OTHER_PROJECT).await;
    World {
        a,
        b,
        home,
        on_a,
        on_b,
        foreign_on_a,
    }
}

// ── the stand-in TUI ─────────────────────────────────────────────────────────────────────────

/// A TUI as a shell script: what it draws, in order, on a screen it has cleared first.
struct ScriptedTui {
    script: String,
    /// The last bytes it writes, which the terminal's capture holds once it has drawn everything.
    last_drawn: Vec<u8>,
}

fn a_scripted_tui() -> ScriptedTui {
    ScriptedTui {
        script: "#!/bin/sh\nprintf '\\033[2J\\033[H'\n".to_string(),
        last_drawn: b"\x1b[2J\x1b[H".to_vec(),
    }
}

impl ScriptedTui {
    fn showing_the_line(mut self, text: &str) -> Self {
        assert!(
            !text.contains('\''),
            "a line is single-quoted in the script: {text:?}"
        );
        self.script
            .push_str(&format!("printf '%s\\r\\n' '{text}'\n"));
        self.last_drawn = text.as_bytes().to_vec();
        self
    }

    /// Move the cursor to the zero-based `row` and `col`.
    fn with_the_cursor_at(mut self, row: u16, col: u16) -> Self {
        let (line, column) = (row + 1, col + 1);
        self.script
            .push_str(&format!("printf '\\033[{line};{column}H'\n"));
        self.last_drawn = format!("\x1b[{line};{column}H").into_bytes();
        self
    }

    fn on_the_alternate_screen(mut self) -> Self {
        self.script
            .push_str("printf '\\033[?1049h\\033[2J\\033[H'\n");
        self.last_drawn = b"\x1b[?1049h\x1b[2J\x1b[H".to_vec();
        self
    }
}

/// A running stand-in TUI; its script lives as long as it does.
struct RunningTui {
    _dir: tempfile::TempDir,
    handle: Arc<PtyHandle>,
}

impl RunningTui {
    /// The offset the terminal's output stream has reached.
    fn output_offset(&self) -> u64 {
        self.handle.capture.lock().unwrap().end_offset()
    }
}

/// Start `tui` as the main terminal of `session_id` on `host`, and wait until it has drawn.
async fn start_main_terminal(host: &Host, session_id: &str, tui: ScriptedTui) -> RunningTui {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("stand_in_tui.sh");
    let script_text = format!("{}exec cat\n", tui.script);
    std::fs::write(&script, script_text).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let handle = host
        .terminals
        .start(
            session_id,
            host.worktree_of(session_id),
            "claude-opus-4-8",
            script.to_str().unwrap(),
            None,
            None,
        )
        .await
        .expect("the main terminal starts");
    let deadline = tokio::time::Instant::now() + DRAW_TIMEOUT;
    loop {
        let drawn = {
            let capture = handle.capture.lock().unwrap();
            capture
                .buffered_bytes()
                .windows(tui.last_drawn.len())
                .any(|window| window == tui.last_drawn.as_slice())
        };
        if drawn {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the stand-in TUI never finished drawing"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    RunningTui { _dir: dir, handle }
}

// ── what the tool answers ────────────────────────────────────────────────────────────────────

/// A `ReadSessionTerminal` answer, read as the agent reads it.
struct ScreenReading(serde_json::Value);

impl ScreenReading {
    fn of(response: &ExecuteToolResponse) -> Self {
        assert!(!response.is_error, "{}", response.error_message);
        Self(serde_json::from_str(&response.result_json).expect("the answer is JSON"))
    }

    fn lines(&self) -> Vec<String> {
        self.0["lines"]
            .as_array()
            .expect("a lines array")
            .iter()
            .map(|line| line.as_str().expect("a text line").to_string())
            .collect()
    }

    fn assert_non_blank_lines(&self, expected: &[&str]) -> &Self {
        let shown: Vec<String> = self
            .lines()
            .into_iter()
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(shown, expected, "the screen's text");
        self
    }

    fn assert_cursor(&self, row: u64, col: u64, visible: bool) -> &Self {
        assert_eq!(
            self.0["cursor"],
            serde_json::json!({ "row": row, "col": col, "visible": visible }),
            "the cursor"
        );
        self
    }

    fn assert_grid(&self, cols: u64, rows: u64) -> &Self {
        assert_eq!(self.0["cols"], cols, "the grid's width");
        assert_eq!(self.0["rows"], rows, "the grid's height");
        assert_eq!(self.lines().len() as u64, rows, "one line per row");
        self
    }

    fn assert_offset(&self, offset: u64) -> &Self {
        assert_eq!(self.0["offset"], offset, "the stream offset");
        self
    }
}

fn assert_refused_saying(response: &ExecuteToolResponse, reason: &str) {
    assert!(
        response.is_error,
        "expected a refusal, got {}",
        response.result_json
    );
    assert!(
        response.error_message.contains(reason),
        "refused, but not for {reason:?}: {}",
        response.error_message
    );
}

impl World {
    async fn call_as(
        &self,
        session_id: &str,
        args: serde_json::Value,
    ) -> Result<ExecuteToolResponse, tddy_rpc::Status> {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                tool_name: "ReadSessionTerminal".to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .map(|r| r.into_inner())
    }

    /// `ReadSessionTerminal`, called by the home session on host-a.
    async fn home_reads(&self, args: serde_json::Value) -> ExecuteToolResponse {
        self.call_as(&self.home, args)
            .await
            .unwrap_or_else(|s| panic!("ReadSessionTerminal is served to the home session: {s}"))
    }
}

fn read_of(session_id: &str) -> serde_json::Value {
    serde_json::json!({ "session_id": session_id })
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_child_running_a_scripted_tui_shows_its_screen_text_and_cursor_through_read_session_terminal(
) {
    // Given a session whose TUI has drawn a dialog and parked the cursor on its second line
    let world = a_world().await;
    let tui = start_main_terminal(
        &world.a,
        &world.on_a,
        a_scripted_tui()
            .showing_the_line("Allow Bash to run `cargo test`?")
            .showing_the_line("  1. Yes")
            .showing_the_line("  2. No")
            .with_the_cursor_at(1, 3),
    )
    .await;

    // When the home session reads its terminal
    let screen = ScreenReading::of(&world.home_reads(read_of(&world.on_a)).await);

    // Then it gets exactly the text, the cursor, the grid and the stream offset the screen model holds
    screen
        .assert_non_blank_lines(&["Allow Bash to run `cargo test`?", "  1. Yes", "  2. No"])
        .assert_cursor(1, 3, true)
        .assert_grid(GRID_COLS, GRID_ROWS)
        .assert_offset(tui.output_offset());
}

#[tokio::test]
async fn a_session_on_another_host_is_read_through_the_forward() {
    // Given a session of the project on host-b, whose TUI has drawn its screen; host-a holds no
    // such session at all
    let world = a_world().await;
    start_main_terminal(
        &world.b,
        &world.on_b,
        a_scripted_tui().showing_the_line("compiling on host-b"),
    )
    .await;

    // When the home session on host-a reads it, naming host-b
    let screen = ScreenReading::of(
        &world
            .home_reads(serde_json::json!({
                "session_id": world.on_b,
                "daemon_instance_id": HOST_B,
            }))
            .await,
    );

    // Then it is host-b's screen
    screen
        .assert_non_blank_lines(&["compiling on host-b"])
        .assert_grid(GRID_COLS, GRID_ROWS);
}

#[tokio::test]
async fn while_the_child_is_on_the_alternate_screen_the_grid_is_the_alternate_screen() {
    // Given a session whose TUI drew on the primary screen, then switched to the alternate one
    let world = a_world().await;
    start_main_terminal(
        &world.a,
        &world.on_a,
        a_scripted_tui()
            .showing_the_line("primary screen text")
            .on_the_alternate_screen()
            .showing_the_line("alternate screen text"),
    )
    .await;

    // When the home session reads its terminal
    let screen = ScreenReading::of(&world.home_reads(read_of(&world.on_a)).await);

    // Then only the alternate screen's text is shown
    screen.assert_non_blank_lines(&["alternate screen text"]);
}

#[tokio::test]
async fn another_projects_session_the_home_session_and_an_unknown_id_are_each_refused_naming_why() {
    // Given a session of another project, the home session itself and an id nobody knows, each
    // with a terminal that has drawn
    let world = a_world().await;
    start_main_terminal(
        &world.a,
        &world.foreign_on_a,
        a_scripted_tui().showing_the_line("screen of another project"),
    )
    .await;
    start_main_terminal(
        &world.a,
        &world.home,
        a_scripted_tui().showing_the_line("screen of the home session itself"),
    )
    .await;

    // When the home session reads each
    let another_project = world.home_reads(read_of(&world.foreign_on_a)).await;
    let itself = world.home_reads(read_of(&world.home)).await;
    let unknown = world.home_reads(read_of(UNKNOWN_SESSION)).await;

    // Then each is refused for its own reason, and none shows a screen
    assert_refused_saying(&another_project, "belongs to another project");
    assert_refused_saying(&itself, "the project-home session is not a target");
    assert_refused_saying(&unknown, "no session");
    assert!(!another_project
        .result_json
        .contains("screen of another project"));
    assert!(!itself.result_json.contains("home session itself"));
}

#[tokio::test]
async fn a_session_without_a_live_terminal_is_refused_with_a_reason() {
    // Given a session of the project that has no terminal running
    let world = a_world().await;

    // When the home session reads its terminal
    let refused = world.home_reads(read_of(&world.on_a)).await;

    // Then it is refused, naming the session and the missing terminal
    assert_refused_saying(&refused, "no live terminal");
    assert_refused_saying(&refused, &world.on_a);
}

#[tokio::test]
async fn a_non_project_home_caller_is_refused_as_permission_denied() {
    // Given an ordinary session of the project with a terminal that has drawn
    let world = a_world().await;
    let other = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;
    start_main_terminal(
        &world.a,
        &world.on_a,
        a_scripted_tui().showing_the_line("a screen an ordinary session must not read"),
    )
    .await;

    // When that ordinary session calls ReadSessionTerminal
    let refusal = world
        .call_as(&other, read_of(&world.on_a))
        .await
        .expect_err("an ordinary session is refused");

    // Then it is refused as permission denied
    assert_eq!(refusal.code(), Code::PermissionDenied);
}

#[tokio::test]
async fn the_tool_is_offered_to_a_project_home_session_and_not_to_any_other() {
    // Given a project-home session and an ordinary session of the same project
    let world = a_world().await;

    // When each is asked which exec tools it is offered
    let to_home = offered_tool_names(&world, &world.home).await;
    let to_ordinary = offered_tool_names(&world, &world.on_a).await;

    // Then only the project-home session is offered ReadSessionTerminal
    assert!(
        to_home.iter().any(|name| name == "ReadSessionTerminal"),
        "offered to the home session: {to_home:?}"
    );
    assert!(
        !to_ordinary.iter().any(|name| name == "ReadSessionTerminal"),
        "offered to an ordinary session: {to_ordinary:?}"
    );
}

/// The tool names `ListExecTools` offers `session_id`, sorted.
async fn offered_tool_names(world: &World, session_id: &str) -> Vec<String> {
    let listed = world
        .a
        .daemon
        .list_exec_tools(Request::direct(ListExecToolsRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect("the session's tools are listed")
        .into_inner();
    let mut names: Vec<String> = listed.tools.into_iter().map(|tool| tool.name).collect();
    names.sort();
    names
}

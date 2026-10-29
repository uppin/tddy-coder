//! Acceptance: a project-home session that asks `ReadSessionTerminal` for `styles` learns how
//! another session's TUI paints its screen: the runs of coloured or attributed text per row, and
//! the rows one inverse or background-coloured run highlights (`#home-inbox` 9/9). A read without
//! `styles` answers exactly what it did before (`#home-inbox` 2/8).
//!
//! A session's TUI is stood in for by a shell script that draws a known screen with real SGR
//! escape sequences and then blocks on stdin, started as the session's main terminal in the
//! daemon's own terminal manager, so the tool reads the same screen model a browser attaching
//! would be shown.
//!
//! Feature doc: docs/ft/daemon/1-WIP/PRD-2026-10-29-terminal-styles.md.
//! Changeset: docs/dev/1-WIP/2026-10-29-home-inbox-terminal-styles.md

mod support;

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::claude_cli_session::PtyHandle;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

/// The grid every PTY the daemon spawns starts at (`tddy_pty::DEFAULT_TERM_{COLS,ROWS}`).
const GRID_COLS: u64 = 220;
const GRID_ROWS: u64 = 24;
/// Where the right pane of the two-pane screen starts, and how wide the left one is.
const LEFT_PANE_COLS: usize = 30;
/// How long the stand-in TUI is given to draw its screen.
const DRAW_TIMEOUT: Duration = Duration::from_secs(15);

/// SGR parameters: inverse video, bold, red foreground, blue background.
const INVERSE: &str = "7";
const BOLD: &str = "1";
const RED: &str = "31";
const BLUE_BACKGROUND: &str = "44";

struct World {
    a: Host,
    b: Host,
    home: String,
    on_a: String,
    on_b: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    World {
        a,
        b,
        home,
        on_a,
        on_b,
    }
}

// ── the stand-in TUI ─────────────────────────────────────────────────────────────────────────

/// One stretch of a drawn line: plain text, or text under SGR parameters.
struct Span {
    sgr: &'static str,
    text: String,
}

fn plain(text: &str) -> Span {
    styled("", text)
}

fn styled(sgr: &'static str, text: &str) -> Span {
    assert!(
        !text.contains(['\'', '%', '\\']),
        "a span is single-quoted in a printf script: {text:?}"
    );
    Span {
        sgr,
        text: text.to_string(),
    }
}

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
    fn showing_the_line(self, text: &str) -> Self {
        self.showing_the_spans(&[plain(text)])
    }

    /// Draw one line made of `spans`, each styled span reset after its text.
    fn showing_the_spans(mut self, spans: &[Span]) -> Self {
        let mut format = String::new();
        let mut arguments = String::new();
        for span in spans {
            if span.sgr.is_empty() {
                format.push_str("%s");
            } else {
                format.push_str(&format!("\\033[{}m%s\\033[0m", span.sgr));
            }
            arguments.push_str(&format!(" '{}'", span.text));
        }
        self.script
            .push_str(&format!("printf '{format}\\r\\n'{arguments}\n"));
        let last = spans.last().expect("a line has a span");
        self.last_drawn = if last.sgr.is_empty() {
            last.text.as_bytes().to_vec()
        } else {
            format!("{}\x1b[0m", last.text).into_bytes()
        };
        self
    }
}

impl ScriptedTui {
    /// Move the cursor to the zero-based `row` and `col`.
    fn with_the_cursor_at(mut self, row: u16, col: u16) -> Self {
        let (line, column) = (row + 1, col + 1);
        self.script
            .push_str(&format!("printf '\\033[{line};{column}H'\n"));
        self.last_drawn = format!("\x1b[{line};{column}H").into_bytes();
        self
    }
}

/// A running stand-in TUI; its script lives as long as it does.
struct RunningTui {
    _dir: tempfile::TempDir,
    _handle: Arc<PtyHandle>,
}

/// Start `tui` as the main terminal of `session_id` on `host`, and wait until it has drawn.
async fn start_main_terminal(host: &Host, session_id: &str, tui: ScriptedTui) -> RunningTui {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("stand_in_tui.sh");
    std::fs::write(&script, format!("{}exec cat\n", tui.script)).unwrap();
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
    RunningTui {
        _dir: dir,
        _handle: handle,
    }
}

// ── what the tool answers ────────────────────────────────────────────────────────────────────

/// A `ReadSessionTerminal` answer, read as the agent reads it.
struct ScreenReading(Value);

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

    /// The `styles` entries of the answer: one `{row, runs}` per row that has a run.
    fn styled_rows(&self) -> &Vec<Value> {
        self.0["styles"]
            .as_array()
            .unwrap_or_else(|| panic!("the answer has a styles array, but it is {}", self.0))
    }

    /// The rows that have any run, ascending.
    fn rows_with_runs(&self) -> Vec<u64> {
        self.styled_rows()
            .iter()
            .map(|entry| entry["row"].as_u64().expect("a row number"))
            .collect()
    }

    fn runs_of_row(&self, row: u64) -> Value {
        self.styled_rows()
            .iter()
            .find(|entry| entry["row"] == row)
            .map(|entry| entry["runs"].clone())
            .unwrap_or_else(|| {
                panic!(
                    "row {row} has no runs; the rows that do: {:?}",
                    self.rows_with_runs()
                )
            })
    }

    fn highlighted_rows(&self) -> Value {
        self.0
            .get("highlighted_rows")
            .unwrap_or_else(|| panic!("the answer has highlighted_rows, but it is {}", self.0))
            .clone()
    }

    fn assert_rows_with_runs(&self, expected: &[u64]) -> &Self {
        assert_eq!(self.rows_with_runs(), expected, "the rows that have runs");
        self
    }

    fn assert_runs(&self, row: u64, expected: Value) -> &Self {
        assert_eq!(self.runs_of_row(row), expected, "the runs of row {row}");
        self
    }

    fn assert_highlighted_rows(&self, expected: &[u64]) -> &Self {
        assert_eq!(
            self.highlighted_rows(),
            json!(expected),
            "the highlighted rows"
        );
        self
    }

    fn assert_line(&self, row: usize, expected: &str) -> &Self {
        assert_eq!(self.lines()[row], expected, "the text of row {row}");
        self
    }

    /// The names of the answer's top-level fields, sorted.
    fn field_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .0
            .as_object()
            .expect("the answer is an object")
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }
}

impl World {
    async fn home_reads(&self, args: Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: "ReadSessionTerminal".to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .unwrap_or_else(|s| panic!("ReadSessionTerminal is served to the home session: {s}"))
            .into_inner()
    }

    /// The home session reads `session_id` asking for its styles.
    async fn home_reads_styles_of(&self, session_id: &str) -> ScreenReading {
        ScreenReading::of(
            &self
                .home_reads(json!({ "session_id": session_id, "styles": true }))
                .await,
        )
    }
}

fn read_of(session_id: &str) -> Value {
    json!({ "session_id": session_id })
}

/// A menu whose second item is the one under the cursor, drawn in inverse video.
fn a_menu_with_the_second_item_inverse() -> ScriptedTui {
    a_scripted_tui()
        .showing_the_line("Pick a branch")
        .showing_the_spans(&[styled(INVERSE, "> develop")])
        .showing_the_line("  release")
}

// ── the tests ────────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_inverse_video_menu_row_is_highlighted_and_its_run_is_inverse_over_the_row() {
    // Given a session whose TUI drew a menu with its second item in inverse video
    let world = a_world().await;
    let _tui =
        start_main_terminal(&world.a, &world.on_a, a_menu_with_the_second_item_inverse()).await;

    // When the home session reads its terminal with styles
    let screen = world.home_reads_styles_of(&world.on_a).await;

    // Then that row is highlighted, and its one run is inverse over the row's nine columns
    screen
        .assert_line(1, "> develop")
        .assert_highlighted_rows(&[1])
        .assert_rows_with_runs(&[1])
        .assert_runs(1, json!([{ "from": 0, "to": 9, "inverse": true }]));
}

#[tokio::test]
async fn a_background_bar_wider_than_its_text_is_highlighted_and_its_run_reaches_past_the_text() {
    // Given a session whose TUI painted a row with a blue background, padded to twenty columns
    let world = a_world().await;
    let _tui = start_main_terminal(
        &world.a,
        &world.on_a,
        a_scripted_tui()
            .showing_the_line("Menu")
            .showing_the_spans(&[styled(BLUE_BACKGROUND, &format!("{:<20}", "  Open file"))]),
    )
    .await;

    // When the home session reads its terminal with styles
    let screen = world.home_reads_styles_of(&world.on_a).await;

    // Then the row's text is trimmed to eleven characters, but its run carries the background and
    // runs to column twenty, and the row is highlighted
    screen
        .assert_line(1, "  Open file")
        .assert_highlighted_rows(&[1])
        .assert_runs(1, json!([{ "from": 0, "to": 20, "bg": "blue" }]));
}

#[tokio::test]
async fn a_red_word_and_a_bold_word_in_an_ordinary_row_are_two_runs_and_do_not_highlight_it() {
    // Given a session whose TUI drew one row with a red word and a bold word among plain text
    let world = a_world().await;
    let _tui = start_main_terminal(
        &world.a,
        &world.on_a,
        a_scripted_tui().showing_the_spans(&[
            plain("status: "),
            styled(RED, "error"),
            plain(" and "),
            styled(BOLD, "warning"),
            plain(" found"),
        ]),
    )
    .await;

    // When the home session reads its terminal with styles
    let screen = world.home_reads_styles_of(&world.on_a).await;

    // Then the two words are runs, with only their own attributes, and no row is highlighted
    screen
        .assert_line(0, "status: error and warning found")
        .assert_rows_with_runs(&[0])
        .assert_runs(
            0,
            json!([
                { "from": 8, "to": 13, "fg": "red" },
                { "from": 18, "to": 25, "bold": true },
            ]),
        )
        .assert_highlighted_rows(&[]);
}

#[tokio::test]
async fn a_red_word_inside_an_inverse_bar_does_not_stop_the_row_being_highlighted() {
    // Given a session whose TUI drew a menu row in inverse video with one word of it also red
    let world = a_world().await;
    let _tui = start_main_terminal(
        &world.a,
        &world.on_a,
        a_scripted_tui()
            .showing_the_line("Pick a branch")
            .showing_the_spans(&[
                styled(INVERSE, "> "),
                styled("7;31", "hotfix"),
                styled(INVERSE, " branch"),
            ]),
    )
    .await;

    // When the home session reads its terminal with styles
    let screen = world.home_reads_styles_of(&world.on_a).await;

    // Then the row is three adjacent inverse runs, the middle one also red, and it is highlighted
    screen
        .assert_line(1, "> hotfix branch")
        .assert_runs(
            1,
            json!([
                { "from": 0, "to": 2, "inverse": true },
                { "from": 2, "to": 8, "fg": "red", "inverse": true },
                { "from": 8, "to": 15, "inverse": true },
            ]),
        )
        .assert_highlighted_rows(&[1]);
}

#[tokio::test]
async fn a_read_without_styles_answers_the_text_only_shape_and_a_styled_read_only_adds_to_it() {
    // Given a session whose TUI drew a menu with an inverse row
    let world = a_world().await;
    let _tui =
        start_main_terminal(&world.a, &world.on_a, a_menu_with_the_second_item_inverse()).await;

    // When the home session reads it without styles, with styles false, and with styles true
    let without = ScreenReading::of(&world.home_reads(read_of(&world.on_a)).await);
    let false_flag = ScreenReading::of(
        &world
            .home_reads(json!({ "session_id": world.on_a, "styles": false }))
            .await,
    );
    let with = world.home_reads_styles_of(&world.on_a).await;

    // Then the plain answers are exactly the text-only fields, and equal to each other
    let text_only = ["cols", "cursor", "lines", "offset", "rows"];
    assert_eq!(without.field_names(), text_only, "a read without styles");
    assert_eq!(
        false_flag.0, without.0,
        "styles: false is the same as no flag"
    );
    // and the styled answer is those same fields, unchanged, plus the two it asked for
    let mut styled_fields: Vec<&str> = text_only.to_vec();
    styled_fields.extend(["highlighted_rows", "styles"]);
    styled_fields.sort();
    assert_eq!(with.field_names(), styled_fields, "a read with styles");
    for field in text_only {
        assert_eq!(with.0[field], without.0[field], "the field {field}");
    }
    with.assert_rows_with_runs(&[1]);
}

#[tokio::test]
async fn a_styled_read_of_a_session_on_another_host_answers_through_the_forward() {
    // Given a session of the project on host-b whose TUI drew a menu with an inverse row; host-a
    // holds no such session at all
    let world = a_world().await;
    let _tui =
        start_main_terminal(&world.b, &world.on_b, a_menu_with_the_second_item_inverse()).await;

    // When the home session on host-a reads it with styles, naming host-b
    let screen = ScreenReading::of(
        &world
            .home_reads(json!({
                "session_id": world.on_b,
                "daemon_instance_id": HOST_B,
                "styles": true,
            }))
            .await,
    );

    // Then it is host-b's screen, with the same highlighted row and run a local read would give
    screen
        .assert_line(1, "> develop")
        .assert_highlighted_rows(&[1])
        .assert_runs(1, json!([{ "from": 0, "to": 9, "inverse": true }]));
}

// ── a crop box ───────────────────────────────────────────────────────────────────────────────

/// A left pane of files and a right pane of branches, whose menu row (row 2) is inverse. The right
/// pane starts at column 30, so on screen the inverse row also holds the left pane's text.
fn a_two_pane_screen() -> ScriptedTui {
    let pane_row =
        |left: &str, right: Span| [styled("", &format!("{left:<LEFT_PANE_COLS$}")), right];
    a_scripted_tui()
        .showing_the_spans(&pane_row("Files", plain("Branches")))
        .showing_the_spans(&pane_row("  src/main.rs", plain("  main")))
        .showing_the_spans(&pane_row("  Cargo.toml", styled(INVERSE, "> develop")))
        .showing_the_spans(&pane_row("  README.md", plain("  release")))
}

/// The right pane's menu: from row 1, column 30, three rows of twenty columns.
fn the_branches_menu() -> Value {
    json!({ "row": 1, "col": 30, "rows": 3, "cols": 20 })
}

fn assert_refused_naming_the_screen(response: &ExecuteToolResponse, why: &str) {
    assert!(
        response.is_error,
        "{why}: expected a refusal, got {}",
        response.result_json
    );
    assert!(
        response
            .error_message
            .contains(&format!("{GRID_COLS} columns by {GRID_ROWS} rows")),
        "{why}: refused, but not naming the screen size: {}",
        response.error_message
    );
}

#[tokio::test]
async fn a_cropped_styled_read_answers_only_the_boxs_text_and_runs_rebased_and_highlights_inside_it(
) {
    // Given a session whose TUI drew two panes, with the right pane's second item inverse; on the
    // whole screen that row also holds the left pane's text
    let world = a_world().await;
    let _tui = start_main_terminal(&world.a, &world.on_a, a_two_pane_screen()).await;
    let whole = world.home_reads_styles_of(&world.on_a).await;
    assert_eq!(
        whole.highlighted_rows(),
        json!([]),
        "on the whole screen no one run covers the row's text"
    );

    // When the home session reads the right pane's box with styles
    let screen = ScreenReading::of(
        &world
            .home_reads(json!({
                "session_id": world.on_a,
                "styles": true,
                "region": the_branches_menu(),
            }))
            .await,
    );

    // Then it gets only the box's rows and columns, the inverse run re-based to the box, and that
    // row highlighted in the box's own row numbers
    assert_eq!(
        screen.lines(),
        ["  main", "> develop", "  release"],
        "the box's text"
    );
    screen
        .assert_rows_with_runs(&[1])
        .assert_runs(1, json!([{ "from": 0, "to": 9, "inverse": true }]))
        .assert_highlighted_rows(&[1]);
    assert_eq!(screen.0["region"], the_branches_menu(), "the region read");
}

#[tokio::test]
async fn a_cropped_read_without_styles_answers_text_the_region_and_a_box_relative_cursor() {
    // Given two sessions drawing the two panes, one with its cursor in the right pane's menu and
    // one with it on the screen's top-left cell
    let world = a_world().await;
    let elsewhere = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;
    let _inside = start_main_terminal(
        &world.a,
        &world.on_a,
        a_two_pane_screen().with_the_cursor_at(2, 32),
    )
    .await;
    let _outside = start_main_terminal(
        &world.a,
        &elsewhere,
        a_two_pane_screen().with_the_cursor_at(0, 0),
    )
    .await;

    // When the home session reads the menu's box of each, without styles
    let read = |session_id: String| {
        let world = &world;
        async move {
            ScreenReading::of(
                &world
                    .home_reads(json!({
                        "session_id": session_id,
                        "region": the_branches_menu(),
                    }))
                    .await,
            )
        }
    };
    let inside = read(world.on_a.clone()).await;
    let outside = read(elsewhere).await;

    // Then each answers the box's text, the region, the whole screen's size and no style fields,
    // and the cursor is relative to the box
    for screen in [&inside, &outside] {
        assert_eq!(
            screen.field_names(),
            ["cols", "cursor", "lines", "offset", "region", "rows"],
            "a cropped read without styles"
        );
        assert_eq!(screen.lines(), ["  main", "> develop", "  release"]);
        assert_eq!(screen.0["region"], the_branches_menu(), "the region read");
        assert_eq!(screen.0["cols"], GRID_COLS, "the whole screen's width");
        assert_eq!(screen.0["rows"], GRID_ROWS, "the whole screen's height");
    }
    assert_eq!(
        inside.0["cursor"],
        json!({ "row": 1, "col": 2, "visible": true, "inside": true }),
        "a cursor inside the box"
    );
    assert_eq!(
        outside.0["cursor"],
        json!({ "row": -1, "col": -30, "visible": true, "inside": false }),
        "a cursor above and left of the box"
    );
}

#[tokio::test]
async fn a_box_past_the_screen_is_clipped_and_echoed_and_an_empty_or_off_screen_box_is_refused() {
    // Given a session whose TUI drew the two panes
    let world = a_world().await;
    let _tui = start_main_terminal(&world.a, &world.on_a, a_two_pane_screen()).await;

    // When the home session reads a box running past the screen's bottom and right edges
    let clipped = ScreenReading::of(
        &world
            .home_reads(json!({
                "session_id": world.on_a,
                "region": { "row": 22, "col": 200, "rows": 10, "cols": 50 },
            }))
            .await,
    );

    // Then the box is clipped to the screen: two rows of twenty columns, and the echo says so
    assert_eq!(
        clipped.0["region"],
        json!({ "row": 22, "col": 200, "rows": 2, "cols": 20 }),
        "the clipped region"
    );
    assert_eq!(
        clipped.lines().len(),
        2,
        "one line per row of the clipped box"
    );

    // And a box with no rows, one with no columns, and ones wholly off the screen are each refused,
    // naming the screen's size
    for (why, region) in [
        (
            "no rows",
            json!({ "row": 0, "col": 0, "rows": 0, "cols": 5 }),
        ),
        (
            "no columns",
            json!({ "row": 0, "col": 0, "rows": 5, "cols": 0 }),
        ),
        (
            "below the screen",
            json!({ "row": 24, "col": 0, "rows": 2, "cols": 5 }),
        ),
        (
            "right of the screen",
            json!({ "row": 0, "col": 220, "rows": 2, "cols": 5 }),
        ),
    ] {
        let refused = world
            .home_reads(json!({ "session_id": world.on_a, "region": region }))
            .await;
        assert_refused_naming_the_screen(&refused, why);
    }
}

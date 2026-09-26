//! Integration tests: a shell command the engine spawns is contained — it cannot reach the
//! parent's standard input, and it cannot outlive its own budget.
//!
//! Both guarantees were missing, and their absence is the whole of the 2026-09-26 session wedge.
//! `tokio::process::Command::output()` sets stdout and stderr but — unlike its `std` counterpart —
//! leaves **stdin inherited**. For a tool call running inside a jail that stdin is
//! `tddy-sandbox-runner --stdio`'s tool-IPC request pipe, so a command that reads stdin becomes a
//! second reader on the daemon→jail channel. `tokio::time::timeout` then drops the future without
//! `kill_on_drop`, so the command survives its own timeout and keeps reading. In the incident a
//! `grep` with no file operand held that pipe for ten minutes, until the 600s in-jail deadline
//! killed the session's tool channel for good.
//!
//! The four spawn sites share the defect and therefore share these tests:
//! `lib.rs` blocking `tool_shell`, `lib.rs` `ShellTaskBody` (background jobs),
//! `shell.rs` `LocalShell::run`, and `lib.rs` `tool_grep` — the last found only on 2026-09-26
//! review, after the first three had been fixed and the sweep declared complete.
//!
//! ## Why the stdin tests re-exec this binary
//!
//! Under `cargo test` the harness process's own standard input is already at end of file, so a
//! spawned `cat` returns immediately **whether or not the engine closed its stdin**. A test run
//! directly in the harness therefore passes while the defect is present — a false negative, and
//! the exact shape of test that let this ship. The stdin cases instead re-exec this binary with
//! fd 0 bound to a pipe that is open and holds data, which is the condition a `--stdio` runner is
//! always in, and assert the command saw none of it.
//!
//! Feature: docs/ft/coder/sandboxed-codebase-mode.md § Nothing the jail runs can read the channel it is served over

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use tddy_task::TaskRegistry;
use tddy_tool_engine::execute_tool;
use tempfile::TempDir;

/// Set on the re-executed child so it runs the spawn instead of the assertions. Its value is
/// `<host>\u{1f}<argument>\u{1f}<block_until_ms>`.
const RUN_AS_SPAWN_HOST: &str = "TDDY_SHELL_CONTAINMENT_SPAWN_HOST";

/// The two spawn hosts the marker selects: one runs a `Shell` call, the other a `Grep` call.
const SHELL_HOST: &str = "Shell";
const GREP_HOST: &str = "Grep";

/// What the parent feeds its child's standard input. If a spawned command can reach it, this
/// string comes back in the command's stdout — and in production it would have been a protocol
/// frame consumed out of the tool-IPC channel.
const BYTES_ON_THE_PARENTS_STDIN: &str = "frame-the-command-must-never-see\n";

/// The child prints its result on this prefix so the parent can find it among harness noise.
const RESULT_PREFIX: &str = "SPAWN_HOST_RESULT ";

/// Long enough that a command still running when its budget expires would finish well after the
/// assertion, and short enough to keep the suite quick.
const SURVIVOR_GRACE: Duration = Duration::from_secs(3);

/// A budget short enough that the one-second command in the survivor test cannot meet it. It has
/// to stay well under that second, so it cannot also be used where a command is expected to
/// *finish* — see [`AMPLE_BUDGET_MS`].
const IMPATIENT_BUDGET_MS: u64 = 200;

/// The budget for the stdin cases, where the command is expected to finish at once and the
/// timeout is the failure signal rather than the assertion.
///
/// Generous on purpose. A command reading an inherited stdin blocks for **ever**, so any budget
/// catches the defect; a tight one merely adds a way for a loaded machine to fail the test for a
/// reason that is not the defect. At 200 ms a `fork`+`exec` of `sh` on a saturated box can lose
/// the race, which was observed once across nine runs.
const AMPLE_BUDGET_MS: u64 = 5_000;

// ─── The spawn host: run one Shell call and report what it saw ───────────────

/// How long the spawn host waits for a background job before calling it stuck. Well above the
/// milliseconds a `cat` on a closed stdin needs, and well below the harness's patience.
const BACKGROUND_AWAIT_MS: u64 = 2_000;

/// Run `command` through the engine and print the outcome as one JSON line. Called only in the
/// re-executed child, whose standard input is an open pipe holding [`BYTES_ON_THE_PARENTS_STDIN`].
///
/// A background call (`block_until_ms == 0`) answers with a receipt rather than an outcome, so the
/// host awaits the job and reports *that* — a receipt on its own says nothing about whether the
/// job could reach the parent's standard input.
fn run_as_spawn_host(command: &str, block_until_ms: u64) -> ! {
    let worktree = TempDir::new().expect("a worktree to run in");
    let registry = TaskRegistry::new();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");

    let reported = runtime.block_on(async {
        let outcome = execute_tool(
            worktree.path(),
            "Shell",
            &serde_json::json!({ "command": command, "block_until_ms": block_until_ms })
                .to_string(),
            &registry,
            "shell-containment-test",
        )
        .await;

        if block_until_ms != 0 {
            return serde_json::json!({
                "result_json": outcome.result_json,
                "is_error": outcome.is_error,
                "error_message": outcome.error_message,
            });
        }

        let receipt = outcome.result_json.clone();
        let awaited = execute_tool(
            worktree.path(),
            "Await",
            &serde_json::json!({ "job_id": outcome.job_id, "timeout_ms": BACKGROUND_AWAIT_MS })
                .to_string(),
            &registry,
            "shell-containment-test",
        )
        .await;
        serde_json::json!({
            "receipt_json": receipt,
            "result_json": awaited.result_json,
            "is_error": awaited.is_error,
            "error_message": awaited.error_message,
        })
    });

    println!("{RESULT_PREFIX}{reported}");
    std::process::exit(0);
}

/// What the stand-in `rg` prints, with whatever it could read from standard input substituted in.
/// `Grep` keeps every `--json` line whose `type` is `match`, so the shim's report arrives through
/// the tool's own result rather than out of band.
const STAND_IN_RG: &str =
    "#!/bin/sh\nprintf '{\"type\":\"match\",\"data\":{\"stdin_seen\":\"%s\"}}\\n' \"$(cat)\"\n";

/// Run one `Grep` call through the engine, with a stand-in `rg` first on `PATH`, and report the
/// outcome the same way [`run_as_spawn_host`] does.
///
/// The property under test belongs to the engine, not to ripgrep: whatever `Grep` spawns must not
/// be handed the caller's standard input. Real ripgrep, given the explicit `.` path operand
/// `tool_grep` passes, never reads standard input at all — so against the real binary an inherited
/// fd 0 is entirely invisible and the test would pass with the defect fully present, which is the
/// same false negative the module header describes. A shim named `rg` that *does* read standard
/// input makes the difference observable: with fd 0 nulled it sees end of file at once and reports
/// an empty string; with fd 0 inherited it blocks on the parent's still-open pipe until the
/// `Grep` budget expires.
fn run_as_grep_spawn_host(pattern: &str) -> ! {
    let worktree = TempDir::new().expect("a worktree to run in");
    let path_head = TempDir::new().expect("a directory to hold the stand-in `rg`");
    let shim = path_head.path().join("rg");
    std::fs::write(&shim, STAND_IN_RG).expect("to write the stand-in `rg`");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))
            .expect("the stand-in `rg` to be executable");
    }
    std::env::set_var(
        "PATH",
        format!(
            "{}:{}",
            path_head.path().display(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );

    let registry = TaskRegistry::new();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");

    let reported = runtime.block_on(async {
        let outcome = execute_tool(
            worktree.path(),
            "Grep",
            &serde_json::json!({ "pattern": pattern }).to_string(),
            &registry,
            "shell-containment-test",
        )
        .await;
        serde_json::json!({
            "result_json": outcome.result_json,
            "is_error": outcome.is_error,
            "error_message": outcome.error_message,
        })
    });

    println!("{RESULT_PREFIX}{reported}");
    std::process::exit(0);
}

/// Re-exec this test binary for `test_name`, with fd 0 bound to an open pipe that already holds
/// data, and return what the spawn host reported.
fn a_shell_call_from_a_process_whose_stdin_is_readable(
    test_name: &str,
    command: &str,
    block_until_ms: u64,
) -> SpawnHostReport {
    a_tool_call_from_a_process_whose_stdin_is_readable(
        test_name,
        SHELL_HOST,
        command,
        block_until_ms,
    )
}

/// The same re-exec for the `Grep` path, which takes a pattern rather than a command and has no
/// caller-settable budget.
fn a_grep_call_from_a_process_whose_stdin_is_readable(
    test_name: &str,
    pattern: &str,
) -> SpawnHostReport {
    a_tool_call_from_a_process_whose_stdin_is_readable(test_name, GREP_HOST, pattern, 0)
}

fn a_tool_call_from_a_process_whose_stdin_is_readable(
    test_name: &str,
    host: &str,
    argument: &str,
    block_until_ms: u64,
) -> SpawnHostReport {
    let mut child = Command::new(std::env::current_exe().expect("this test binary"))
        .args(["--exact", test_name, "--nocapture", "--test-threads", "1"])
        .env(
            RUN_AS_SPAWN_HOST,
            format!("{host}\u{1f}{argument}\u{1f}{block_until_ms}"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the spawn host to start");

    // Written but deliberately NOT closed: a pipe at end of file would let an inherited stdin look
    // like a closed one, which is the false negative this whole arrangement exists to avoid.
    let mut stdin = child.stdin.take().expect("the spawn host's stdin");
    stdin
        .write_all(BYTES_ON_THE_PARENTS_STDIN.as_bytes())
        .expect("to prime the spawn host's stdin");
    stdin.flush().expect("to flush the spawn host's stdin");

    let output = child.wait_with_output().expect("the spawn host to finish");
    drop(stdin);

    // The harness writes `test <name> ... ` with no newline before the spawn host's own output,
    // so the report is found anywhere in a line rather than at its start.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find_map(|l| {
            l.split_once(RESULT_PREFIX)
                .map(|(_, rest)| rest.to_string())
        })
        .unwrap_or_else(|| panic!("the spawn host reported no outcome; it printed: {stdout}"));
    SpawnHostReport {
        reported: serde_json::from_str(&line).expect("the spawn host's report is JSON"),
    }
}

struct SpawnHostReport {
    reported: serde_json::Value,
}

impl SpawnHostReport {
    fn timed_out(&self) -> bool {
        self.reported["is_error"].as_bool().unwrap_or(false)
            && self.reported["error_message"]
                .as_str()
                .unwrap_or_default()
                .contains("timed out")
    }

    fn error_message(&self) -> &str {
        self.reported["error_message"].as_str().unwrap_or_default()
    }

    fn result(&self) -> serde_json::Value {
        serde_json::from_str(
            self.reported["result_json"]
                .as_str()
                .expect("the spawn host reports the raw result"),
        )
        .unwrap_or(serde_json::Value::Null)
    }

    /// The background call's own answer, before the job was awaited.
    fn receipt(&self) -> serde_json::Value {
        serde_json::from_str(
            self.reported["receipt_json"]
                .as_str()
                .expect("a background report carries the receipt"),
        )
        .unwrap_or(serde_json::Value::Null)
    }

    /// Whether the awaited job reached a terminal status within the host's patience.
    fn job_completed(&self) -> bool {
        self.result()["completed"].as_bool().unwrap_or(false)
    }

    fn stdout(&self) -> String {
        self.result()["stdout"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    fn exit_code(&self) -> i64 {
        self.result()["exit_code"].as_i64().unwrap_or(i64::MIN)
    }

    /// What the stand-in `rg` said it could read from standard input, as `Grep` relayed it.
    fn stdin_the_grep_child_saw(&self) -> String {
        self.result()["matches"][0]["data"]["stdin_seen"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }
}

/// The child half of every re-exec test: if the marker is set, do the spawn and exit before the
/// assertions below are ever reached.
fn serve_if_spawn_host() {
    let Ok(spec) = std::env::var(RUN_AS_SPAWN_HOST) else {
        return;
    };
    let mut fields = spec.split('\u{1f}');
    let host = fields.next().expect("a host");
    let argument = fields.next().expect("an argument");
    let budget = fields.next().expect("a budget");
    match host {
        SHELL_HOST => run_as_spawn_host(argument, budget.parse().expect("a numeric budget")),
        GREP_HOST => run_as_grep_spawn_host(argument),
        other => panic!("unknown spawn host {other}"),
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// The incident's root cause, as one assertion.
///
/// `cat` with no file operand reads standard input. With stdin closed it sees end of file at once
/// and exits 0 having printed nothing. With stdin inherited from a `--stdio` runner it reads the
/// frames meant for the runner and then blocks until its budget expires.
#[test]
fn a_shell_command_cannot_read_the_parents_standard_input() {
    serve_if_spawn_host();

    // Given a parent process whose standard input is an open pipe holding a frame
    // When it runs `cat`, which reads standard input until end of file
    let report = a_shell_call_from_a_process_whose_stdin_is_readable(
        "a_shell_command_cannot_read_the_parents_standard_input",
        "cat",
        AMPLE_BUDGET_MS,
    );

    // Then the command saw end of file at once, and none of the parent's frame reached it
    assert!(
        !report.timed_out(),
        "`cat` blocked on an inherited stdin instead of seeing EOF: {}",
        report.error_message()
    );
    assert_eq!(
        report.stdout(),
        "",
        "the command read the parent's standard input; in a jail those bytes are tool-IPC frames"
    );
    assert_eq!(report.exit_code(), 0, "`cat` on a closed stdin exits 0");
}

/// `ShellTaskBody` is the third spawn site and has the same defect. A background job is exactly
/// where a stolen channel is least likely to be noticed, because nothing is waiting on it.
#[test]
fn a_background_shell_job_cannot_read_the_parents_standard_input_either() {
    serve_if_spawn_host();

    // Given the same parent, and a background job — `block_until_ms: 0` — that echoes whatever it
    // can read from standard input
    // When it is started and then awaited through the job registry
    let report = a_shell_call_from_a_process_whose_stdin_is_readable(
        "a_background_shell_job_cannot_read_the_parents_standard_input_either",
        "printf 'read[%s]' \"$(cat)\"",
        0,
    );

    // Then the call itself answered with a receipt — the background path's own contract
    assert!(
        report.receipt()["job_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "a background job must answer with an addressable job_id, got: {}",
        report.receipt()
    );

    // And the job ran to completion instead of parking forever on a stream it must never reach
    assert!(
        report.job_completed(),
        "the background job never finished; it is still blocked on an inherited stdin"
    );

    // And it read nothing, because its standard input was closed rather than inherited
    assert_eq!(
        report.stdout(),
        "read[]",
        "the background job read the parent's standard input; in a jail those bytes are tool-IPC \
         frames, and nothing is waiting on a background job to notice they went missing"
    );
}

/// A timeout that only stops waiting is not a timeout. The dropped future leaves the child
/// running, which is how the incident's `grep` outlived its own error message by ten minutes.
///
/// This one needs no spawn host: the survivor is observable from the harness directly.
#[tokio::test]
async fn a_shell_command_that_outlives_its_budget_leaves_no_descendant_running() {
    // Given a command whose *grandchild* would leave a mark one second after the budget expires
    let worktree = TempDir::new().expect("a worktree to run in");
    let marker = worktree.path().join("the-command-was-still-running");

    // When the call exceeds its budget
    let outcome = execute_tool(
        worktree.path(),
        "Shell",
        &serde_json::json!({
            "command": "( sleep 1; touch the-command-was-still-running ) & wait",
            "block_until_ms": IMPATIENT_BUDGET_MS,
        })
        .to_string(),
        &TaskRegistry::new(),
        "shell-containment-test",
    )
    .await;
    assert!(
        outcome.is_error && outcome.error_message.contains("timed out"),
        "the call was supposed to exceed its budget, but returned: {}",
        outcome.result_json
    );

    // Then nothing from it is still running once the mark would have been made
    tokio::time::sleep(SURVIVOR_GRACE).await;
    assert!(
        !marker.exists(),
        "a descendant survived the timeout and touched {}; the whole process group must be \
         signalled, not just the direct child",
        marker.display()
    );
}

/// `tool_grep` is the fourth spawn site, and the one the first sweep missed. It ran
/// `tokio::process::Command::new("rg")` directly: stdin inherited, no `kill_on_drop`, no process
/// group, and — unlike the three shell paths — no budget of any kind, so a search that never
/// finished held the tool-IPC pipe until the in-jail deadline killed the channel.
///
/// `Grep` is an advertised tool dispatched through this engine, so it runs on exactly the in-jail
/// path the incident came from.
#[test]
fn the_process_grep_spawns_cannot_read_the_parents_standard_input() {
    serve_if_spawn_host();

    // Given a parent process whose standard input is an open pipe holding a frame
    // When it runs a `Grep`, whose child reads standard input
    let report = a_grep_call_from_a_process_whose_stdin_is_readable(
        "the_process_grep_spawns_cannot_read_the_parents_standard_input",
        "anything",
    );

    // Then the child saw end of file at once rather than parking on the pipe until its budget ran
    // out — `Grep` had no budget at all before this, so with fd 0 inherited it parked for ever
    assert!(
        !report.timed_out(),
        "the process `Grep` spawned blocked on an inherited stdin: {}",
        report.error_message()
    );

    // And none of the parent's frame reached it; in a jail those bytes are tool-IPC frames, and
    // whatever a search consumes from that channel the runner never sees
    assert_eq!(
        report.stdin_the_grep_child_saw(),
        "",
        "the process `Grep` spawned read the parent's standard input"
    );
}

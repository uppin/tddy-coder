//! `tddy-tools restructure` against a warm index daemon, and the cold path it must leave alone.
//!
//! Contract: docs/ft/coder/warm-code-intelligence-daemon.md § Running it
//!          § Opt-in client wiring in `tddy-tools`
//!
//! Every test here drives the real `tddy-tools` binary as a child process rather than calling into
//! it, for one reason: the behaviour under test *is* what a process does with an environment
//! variable, and `std::env::set_var` is process-global while these tests run in parallel threads —
//! a test that set `TDDY_INDEX_SOCKET` in its own process would be setting it for every other test
//! in this binary too.
//!
//! The warm half runs a real `tddy-index-daemon` over a real AF_UNIX socket. The operations chosen
//! for it are the two that need no language server — a shallow `check` and a plan `status` — so the
//! gate stays deterministic and fast; a warm *index* costs minutes per run and is a production
//! test by `docs/dev/guides/testing.md`'s own definition.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

/// Long enough for a debug-profile binary to bind a socket on a loaded machine, short enough that
/// a daemon which never comes up fails the suite instead of stalling it.
const THE_DAEMON_SHOULD_COME_UP_WITHIN: Duration = Duration::from_secs(30);

// ─── The binaries under test ───────────────────────────────────────────────────────────────

/// The index daemon, resolved the way this repo resolves a sibling binary: the cargo-provided path
/// first, then a sibling of the test executable with the `deps/` hop an integration test needs,
/// then the bare name on `PATH`. `CARGO_BIN_EXE_tddy-index-daemon` is not set for this crate's
/// tests — the daemon belongs to another package — so the sibling lookup is what resolves.
fn the_index_daemon() -> PathBuf {
    if let Some(from_cargo) = std::env::var_os("CARGO_BIN_EXE_tddy-index-daemon") {
        return PathBuf::from(from_cargo);
    }
    let mut candidate = std::env::current_exe().expect("the test executable's own path");
    candidate.pop();
    if candidate.ends_with("deps") {
        candidate.pop();
    }
    let sibling = candidate.join("tddy-index-daemon");
    if sibling.is_file() {
        return sibling;
    }
    PathBuf::from("tddy-index-daemon")
}

/// A `tddy-tools restructure` run standing in `workspace`, with no ambient index socket.
///
/// The variable is removed rather than left alone so a developer who has run `./run-index-daemon`
/// in their own shell does not change what this suite tests.
fn a_restructure_run_in(workspace: &Path) -> Command {
    let mut run = Command::new(env!("CARGO_BIN_EXE_tddy-tools"));
    run.current_dir(workspace);
    run.env_remove("TDDY_INDEX_SOCKET");
    run.env_remove("TDDY_SOCKET");
    run.arg("restructure");
    run
}

/// A `PATH` with nothing on it, so a run that reaches for a language server fails at the spawn
/// instead of starting a real rust-analyzer.
///
/// This is how the cold path's reach for its own server is made observable in under a second: the
/// nix dev shell *does* provide rust-analyzer, and a run that found it would index this repo for
/// minutes before saying anything. `tddy-tools` itself is invoked by absolute path and the commands
/// under test shell out to nothing, so an empty `PATH` changes nothing else about the run.
fn with_no_language_server_anywhere(run: &mut Command, nowhere: &Path) {
    run.env("PATH", nowhere);
}

// ─── The workspace and the plan every test works against ───────────────────────────────────

/// A tree holding one twelve-line module and one three-line module — the fixture the cold path's
/// own budget test uses, so the two suites are asserting the same numbers.
fn a_workspace_of_a_long_module_and_a_short_one() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(
        workspace.path().join("src/big.rs"),
        "// a line\n".repeat(12),
    )
    .expect("the long module");
    std::fs::write(
        workspace.path().join("src/small.rs"),
        "// a line\n".repeat(3),
    )
    .expect("the short module");
    workspace
}

/// A plan renaming one symbol in each of the two modules, with an empty snapshot so nothing has to
/// match the tree.
fn a_plan_naming_both_modules(root: &Path) -> PathBuf {
    let plan = root.join("plan.jsonl");
    std::fs::write(
        &plan,
        r#"{"v":1,"snapshot":{}}
{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/big.rs","path":"Registry"},"name":"HostRegistry"}
{"op":"rename_symbol","anchor":{"kind":"symbol","file":"src/small.rs","path":"Clock"},"name":"HostClock"}
"#,
    )
    .expect("the plan");
    plan
}

// ─── A real daemon, on a real socket ───────────────────────────────────────────────────────

/// A `tddy-index-daemon` serving `code_index.CodeIndexService` over an AF_UNIX socket, stopped when
/// the test that started it ends.
struct WarmIndexDaemon {
    process: Child,
    socket: PathBuf,
    /// The socket's directory, held so it outlives the daemon holding the socket.
    _home: tempfile::TempDir,
}

impl WarmIndexDaemon {
    fn socket(&self) -> &Path {
        &self.socket
    }
}

impl Drop for WarmIndexDaemon {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Start a daemon and wait until it says which socket it is listening on.
///
/// The announcement is the readiness contract: the daemon logs `listening on …` to stderr *after*
/// the bind, so a client that has seen the line cannot lose a race with the listener.
fn a_warm_index_daemon() -> WarmIndexDaemon {
    let home = tempfile::tempdir().expect("a directory for the daemon's socket");
    let socket = home.path().join("index.sock");
    let mut process = Command::new(the_index_daemon())
        .arg("--grpc-uds")
        .arg(&socket)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|failure| {
            panic!(
                "the index daemon at {} did not start: {failure}",
                the_index_daemon().display()
            )
        });

    let announcements = lines_announcing_a_listener(process.stderr.take().expect("daemon stderr"));
    let announced = announcements
        .recv_timeout(THE_DAEMON_SHOULD_COME_UP_WITHIN)
        .expect("the daemon announced the socket it is listening on");
    assert!(
        announced.contains(&socket.to_string_lossy().to_string()),
        "the daemon announced a listener that is not the socket it was given: {announced}"
    );

    WarmIndexDaemon {
        process,
        socket,
        _home: home,
    }
}

/// The daemon's `listening on …` lines, on a channel, with the rest of its stderr drained so a
/// full pipe cannot wedge the process this suite is testing against.
fn lines_announcing_a_listener(stderr: std::process::ChildStderr) -> Receiver<String> {
    let (announced, announcements) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if line.contains("listening on") {
                let _ = announced.send(line);
            }
        }
    });
    announcements
}

// ─── Reading what a run said ───────────────────────────────────────────────────────────────

/// Every line a run wrote to stdout, which is the console a developer reads its answer from.
fn console(output: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn narration(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

// ─── The tests ─────────────────────────────────────────────────────────────────────────────

/// The streaming half: a `check` is a server-streaming RPC whose notes and findings have to become
/// the same console the cold path writes, line for line.
#[test]
fn restructure_runs_against_the_warm_daemon_when_the_socket_variable_is_set() {
    // Given a plan naming a twelve-line module and a three-line one, and a warm daemon
    let workspace = a_workspace_of_a_long_module_and_a_short_one();
    let plan = a_plan_naming_both_modules(workspace.path());
    let daemon = a_warm_index_daemon();

    // When the plan is checked against a ten-line budget with the socket variable naming it
    let run = a_restructure_run_in(workspace.path())
        .env("TDDY_INDEX_SOCKET", daemon.socket())
        .args([
            "check",
            plan.to_str().expect("the plan path"),
            "--budget",
            "10",
        ])
        .output()
        .expect("the run completes");

    // Then the console is exactly what the cold path writes for the same check
    assert_eq!(
        console(&run),
        vec![
            "budget: 1 of 2 file(s) over 10 production lines",
            "budget: src/big.rs is 12 production lines, 2 over",
            "no findings",
        ],
        "the daemon's events were not rendered as the cold path renders them; stderr was: {}",
        narration(&run)
    );
    // and the run names the daemon that served it, so a developer can tell a warm run from a cold
    // one rather than having to guess which they got
    assert!(
        narration(&run).contains(&format!(
            "the warm index daemon at {}",
            daemon.socket().display()
        )),
        "the run did not say which daemon served it: {}",
        narration(&run)
    );
    assert!(
        run.status.success(),
        "a check with no findings must exit zero; stderr was: {}",
        narration(&run)
    );
}

/// The unary half, over the same socket: `PlanStatus` carries no stream, and its four counts have
/// to reach the console in the order and the wording the cold path uses.
#[test]
fn a_plans_journal_is_counted_by_the_warm_daemon_when_the_socket_variable_is_set() {
    // Given a two-operation plan whose journal holds nothing, and a warm daemon
    let workspace = a_workspace_of_a_long_module_and_a_short_one();
    let plan = a_plan_naming_both_modules(workspace.path());
    let daemon = a_warm_index_daemon();

    // When its status is asked for with the socket variable naming that daemon
    let run = a_restructure_run_in(workspace.path())
        .env("TDDY_INDEX_SOCKET", daemon.socket())
        .args(["status", plan.to_str().expect("the plan path")])
        .output()
        .expect("the run completes");

    // Then all four counts reach the console, with both operations pending
    assert_eq!(
        console(&run),
        vec!["completed 0", "in_flight 0", "failed 0", "pending 2"],
        "the daemon's counts were not rendered as the cold path renders them; stderr was: {}",
        narration(&run)
    );
    // and the daemon is what answered. The cold path produces these same four lines — that is the
    // point of the feature — so the console alone cannot tell the two apart, and the line naming
    // the endpoint is what does.
    assert!(
        narration(&run).contains(&format!(
            "the warm index daemon at {}",
            daemon.socket().display()
        )),
        "the run did not say which daemon served it: {}",
        narration(&run)
    );
    assert!(
        run.status.success(),
        "an answered status must exit zero; stderr was: {}",
        narration(&run)
    );
}

/// The whole reason the wiring is opt-in: with the variable unset, today's path runs, which means
/// this process starts a language server of its own.
#[test]
fn restructure_spawns_its_own_language_server_when_the_socket_variable_is_unset() {
    // Given an anchors run — which resolves through a language server — and no rust-analyzer
    // anywhere this run can reach
    let workspace = a_workspace_of_a_long_module_and_a_short_one();
    let nowhere = tempfile::tempdir().expect("an empty directory to use as a PATH");

    // When it runs with no index socket named at all
    let mut run = a_restructure_run_in(workspace.path());
    with_no_language_server_anywhere(&mut run, nowhere.path());
    let run = run
        .args(["anchors", "src/big.rs", "--items", "Registry"])
        .output()
        .expect("the run completes");

    // Then the run reached for its own rust-analyzer, and asked no daemon for anything
    assert!(
        narration(&run).contains("rust-analyzer LSP"),
        "the run did not start a language server of its own: {}",
        narration(&run)
    );
    assert!(
        !narration(&run).contains("warm index daemon"),
        "an unset socket variable must not reach a daemon: {}",
        narration(&run)
    );
}

/// Empty-as-unset, the rule `LIVEKIT_TESTKIT_WS_URL` states
/// (`packages/tddy-livekit-testkit/src/livekit_testkit.rs:78-88`): a variable exported with no
/// value is a variable nobody set, and treating it as a socket path would turn `export
/// TDDY_INDEX_SOCKET=` — the obvious way to turn the feature back off — into a refusal.
#[test]
fn an_empty_socket_variable_is_treated_as_unset() {
    // Given the same anchors run, and no rust-analyzer anywhere it can reach
    let workspace = a_workspace_of_a_long_module_and_a_short_one();
    let nowhere = tempfile::tempdir().expect("an empty directory to use as a PATH");

    // When it runs with the socket variable exported with no value
    let mut run = a_restructure_run_in(workspace.path());
    with_no_language_server_anywhere(&mut run, nowhere.path());
    let run = run
        .env("TDDY_INDEX_SOCKET", "")
        .args(["anchors", "src/big.rs", "--items", "Registry"])
        .output()
        .expect("the run completes");

    // Then it took the cold path, exactly as an unset variable does
    assert!(
        narration(&run).contains("rust-analyzer LSP"),
        "an empty socket variable did not fall through to the cold path: {}",
        narration(&run)
    );
    assert!(
        !narration(&run).contains("warm index daemon"),
        "an empty socket variable must not be read as a socket path: {}",
        narration(&run)
    );
}

/// A set-but-unreachable socket is an error, not a hint.
///
/// The operation chosen is one the cold path *would* have answered — a plan status needs no
/// language server — so a silent fall-back would look like a successful warm run while the daemon
/// the developer thought they were using was not there. That is the fallback `CLAUDE.md` forbids
/// without explicit consent, and it would hide a misconfigured daemon indefinitely.
#[test]
fn reports_a_socket_variable_pointing_at_nothing_rather_than_falling_back() {
    // Given a socket path nothing is serving on, and a status run the cold path would answer
    let workspace = a_workspace_of_a_long_module_and_a_short_one();
    let plan = a_plan_naming_both_modules(workspace.path());
    let nowhere = tempfile::tempdir().expect("a directory holding no socket");
    let absent = nowhere.path().join("index.sock");

    // When the socket variable names it
    let run = a_restructure_run_in(workspace.path())
        .env("TDDY_INDEX_SOCKET", &absent)
        .args(["status", plan.to_str().expect("the plan path")])
        .output()
        .expect("the run completes");

    // Then the run fails, naming the socket it could not reach
    assert!(
        !run.status.success(),
        "an unreachable daemon must fail the run; stdout was: {:?}",
        console(&run)
    );
    assert!(
        narration(&run).contains(&absent.to_string_lossy().to_string()),
        "the refusal did not name the socket it could not reach: {}",
        narration(&run)
    );
    // and it answered nothing: the four counts the cold path would have produced are the evidence
    // of a fall-back, so their absence is what makes this test about the fallback rather than
    // about the error message
    assert_eq!(
        console(&run),
        Vec::<String>::new(),
        "an unreachable daemon was silently answered from the cold path"
    );
}

// ─── The narration a run writes beside its answer ──────────────────────────────────────────

/// The three steps a check of this suite's two-operation plan narrates, in the shape #500 gave them.
///
/// The elapsed value in each stamp is elided rather than matched: it is measured while the run
/// happens — `+0ms` on an idle machine, `+14ms` on a loaded one — so no literal could hold it.
/// Everything around it is compared exactly, which is what makes this pin the *shape* both front
/// ends have to agree on rather than a duration neither controls.
const THE_STEPS_A_CHECK_OF_BOTH_MODULES_NARRATES: [&str; 3] = [
    "   indexing (+…): check: 2 operation(s)",
    "   indexing (+…): op 0 of 2: static check RenameSymbol in `src/big.rs`",
    "   indexing (+…): op 1 of 2: static check RenameSymbol in `src/small.rs`",
];

/// The server's narration of how far it got, each line's elapsed value elided.
fn indexing_account(output: &std::process::Output) -> Vec<String> {
    narration(output)
        .lines()
        .filter(|line| line.trim_start().starts_with("indexing"))
        .map(with_the_stamp_elided)
        .collect()
}

/// One narration line, with the elapsed duration inside its stamp replaced by an ellipsis.
///
/// A line carrying no stamp at all fails here rather than being passed through, because an
/// unstamped line is the defect this pins: on a six-to-ten-minute crate-graph load the stamp is the
/// only thing that distinguishes a run making progress from one that has hung.
fn with_the_stamp_elided(line: &str) -> String {
    let (before, stamped) = line
        .split_once("(+")
        .unwrap_or_else(|| panic!("this narration line carries no elapsed stamp: {line}"));
    let (_elapsed, after) = stamped
        .split_once(')')
        .unwrap_or_else(|| panic!("this narration line's stamp is never closed: {line}"));
    format!("{before}(+…){after}")
}

/// The stderr counterpart of `restructure_runs_against_the_warm_daemon_when_the_socket_variable_is_set`.
///
/// That test pins the whole *stdout* vector against the cold path's literals, which is why this
/// drift went unnoticed: the stamp #500 introduced lives on stderr. It is the whole point of #500 —
/// on a seven-minute load it is the only thing distinguishing progress from a hang — so a client of
/// the daemon has to be given it too.
#[test]
fn the_warm_paths_narration_stamps_every_step_as_the_cold_paths_does() {
    // Given a plan naming a twelve-line module and a three-line one, and a warm daemon
    let workspace = a_workspace_of_a_long_module_and_a_short_one();
    let plan = a_plan_naming_both_modules(workspace.path());
    let daemon = a_warm_index_daemon();

    // When the same check runs against that daemon, and again with no daemon at all
    let warm = a_restructure_run_in(workspace.path())
        .env("TDDY_INDEX_SOCKET", daemon.socket())
        .args([
            "check",
            plan.to_str().expect("the plan path"),
            "--budget",
            "10",
        ])
        .output()
        .expect("the warm run completes");
    let cold = a_restructure_run_in(workspace.path())
        .args([
            "check",
            plan.to_str().expect("the plan path"),
            "--budget",
            "10",
        ])
        .output()
        .expect("the cold run completes");

    // Then both narrate the same three steps, each stamped with the time since the step before it
    assert_eq!(
        indexing_account(&warm),
        THE_STEPS_A_CHECK_OF_BOTH_MODULES_NARRATES
            .map(String::from)
            .to_vec(),
        "the warm path's narration is not the cold path's; its whole stderr was: {}",
        narration(&warm)
    );
    assert_eq!(
        indexing_account(&cold),
        THE_STEPS_A_CHECK_OF_BOTH_MODULES_NARRATES
            .map(String::from)
            .to_vec(),
        "the cold path's narration changed shape; its whole stderr was: {}",
        narration(&cold)
    );
}

// ─── Snapshot routing, and warming a root ──────────────────────────────────────────────────

/// A v2 plan whose one operation is anchored by an **item**, which is the only kind of plan whose
/// snapshot needs a language server: the fingerprint is made up, because what is under test is
/// where the run goes, not what it finds.
fn an_item_anchored_plan_in(root: &Path) -> PathBuf {
    let plan = root.join("item-plan.jsonl");
    std::fs::write(
        &plan,
        concat!(
            "{\"v\":2,\"files\":{}}\n",
            "{\"op\":\"rename_symbol\",\"anchor\":{\"kind\":\"item\",\"item\":\"t::big::Registry\",",
            "\"file\":\"src/big.rs\",\"fingerprint\":\"sha256:0000\"},\"name\":\"HostRegistry\"}\n",
        ),
    )
    .expect("the plan");
    plan
}

/// A one-crate cargo workspace small enough for a real rust-analyzer to index in seconds.
fn a_tiny_cargo_workspace() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(
        workspace.path().join("Cargo.toml"),
        "[package]\nname = \"tiny\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("a manifest");
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn answer() -> u32 {\n    42\n}\n",
    )
    .expect("a source file");
    workspace
}

/// A snapshot of an item-anchored plan has to re-resolve its anchors, which only a language server
/// can do — so with a warm daemon holding one, the run goes there instead of paying a cold index of
/// its own.
///
/// What is asserted is the route, not the answer: the narration names the daemon and never says it
/// is acquiring a client of its own. That is deterministic whatever the daemon finds in a
/// workspace that has no crate in it.
#[test]
fn routes_a_snapshot_of_an_item_anchored_plan_at_the_warm_daemon() {
    // Given a plan anchored by item, a warm daemon, and no language server on this client's PATH
    let workspace = a_workspace_of_a_long_module_and_a_short_one();
    let plan = an_item_anchored_plan_in(workspace.path());
    let daemon = a_warm_index_daemon();
    let nowhere = tempfile::tempdir().expect("an empty PATH");
    let mut run = a_restructure_run_in(workspace.path());
    with_no_language_server_anywhere(&mut run, nowhere.path());

    // When the plan is snapshotted with the socket variable naming the daemon
    let output = run
        .env("TDDY_INDEX_SOCKET", daemon.socket())
        .args(["snapshot", plan.to_str().expect("the plan path")])
        .output()
        .expect("the run completes");

    // Then it ran against the daemon, and never started a server of its own
    let said = narration(&output);
    assert!(
        said.contains("running against the warm index daemon"),
        "the snapshot did not go to the daemon; stderr was: {said}"
    );
    assert!(
        !said.contains("acquiring shared rust-analyzer client"),
        "the snapshot started a rust-analyzer of its own beside the warm one; stderr was: {said}"
    );
}

/// A plan with no item anchors needs no server at all, so the route that is right for it is the
/// one it already has: the hash of a file this process can read is not worth a socket.
#[test]
fn keeps_a_snapshot_of_a_plan_with_no_item_anchors_in_process() {
    // Given a plan anchored by symbol, and a warm daemon
    let workspace = a_workspace_of_a_long_module_and_a_short_one();
    let plan = a_plan_naming_both_modules(workspace.path());
    let daemon = a_warm_index_daemon();

    // When it is snapshotted with the socket variable naming the daemon
    let output = a_restructure_run_in(workspace.path())
        .env("TDDY_INDEX_SOCKET", daemon.socket())
        .args(["snapshot", plan.to_str().expect("the plan path")])
        .output()
        .expect("the run completes");

    // Then it succeeds without ever naming the daemon
    assert!(
        output.status.success(),
        "a snapshot of a plan with no item anchors failed; stderr was: {}",
        narration(&output)
    );
    assert!(
        !narration(&output).contains("running against the warm index daemon"),
        "a plan that needs no server was sent to the daemon"
    );
}

/// `warm` exists to load a root's crate graph into a daemon, so without a daemon it has nothing to
/// do — and says how to get one, rather than starting a rust-analyzer in-process that would be
/// thrown away when the command exits.
#[test]
fn refuses_to_warm_without_a_daemon_and_names_the_script_that_starts_one() {
    // Given a workspace and no socket variable
    let workspace = a_tiny_cargo_workspace();

    // When `restructure warm` runs
    let output = a_restructure_run_in(workspace.path())
        .arg("warm")
        .output()
        .expect("the run completes");

    // Then it refuses, naming the script
    let said = format!("{}{}", console(&output).join("\n"), narration(&output));
    assert!(
        !output.status.success(),
        "`warm` claimed success with no daemon to warm: {said}"
    );
    assert!(
        said.contains("run-index-daemon"),
        "the refusal did not name `./run-index-daemon`: {said}"
    );
}

/// The claim the subcommand exists for, against a real rust-analyzer: after `warm`, the daemon holds
/// the root, so the first real request does not pay for loading it.
///
/// A production test by `docs/dev/guides/testing.md`: it boots a real rust-analyzer. Run it
/// deliberately:
///
/// ```bash
/// ./dev cargo test -p tddy-tools --test index_daemon_client_acceptance -- --ignored --test-threads=1
/// ```
#[test]
#[ignore = "boots a real rust-analyzer and loads a real crate graph; needs a toolchain"]
fn leaves_the_root_warm_on_the_daemon_it_was_asked_to_warm() {
    // Given a tiny cargo workspace and a daemon holding no roots
    let workspace = a_tiny_cargo_workspace();
    let daemon = a_warm_index_daemon();

    // When `restructure warm` runs against it
    let output = a_restructure_run_in(workspace.path())
        .env("TDDY_INDEX_SOCKET", daemon.socket())
        .arg("warm")
        .output()
        .expect("the run completes");

    // Then it succeeds, and the daemon says it holds one warm workspace
    assert!(
        output.status.success(),
        "`warm` failed; stderr was: {}",
        narration(&output)
    );
    let ping = Command::new(the_index_daemon())
        .arg("--ping")
        .arg(daemon.socket())
        .output()
        .expect("the ping completes");
    assert!(
        String::from_utf8_lossy(&ping.stdout).contains("1 warm workspace(s)"),
        "the daemon does not hold the root after `warm`: {}",
        String::from_utf8_lossy(&ping.stdout)
    );
}

//! A run leaves a record of every process it starts, and of how each one ended.
//!
//! A `restructure` run starts `git`, `cargo check`, `rustfmt` and, on the cold command line, a
//! language server. Until a record exists, a process killed by a signal, a `cargo check` that
//! blocked and a daemon that vanished leave no line anywhere. These suites drive a real apply over
//! a temporary workspace (real `git`, `cargo`, `rustfmt`; `fake_lsp` as the language server, so no
//! rust-analyzer) and read what the record says, as values from a collecting observer or as JSON
//! lines from the file sink.

mod harness;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use harness::{
    a_workspace_whose_origin_build_script_never_finishes,
    a_workspace_whose_test_binary_stands_alone, cancelling_once_a_process_has_started,
    moving_the_test_binary_in_a_group_recording, moving_the_test_binary_recording, CollectedSpawns,
    KeptProcess, ORIGIN_LIB,
};
use serde_json::Value;
use tddy_code_restructuring::backends::rust::RustBackend;
use tddy_code_restructuring::registry::{LanguageBackend, Workspace};
use tddy_code_restructuring::runner::RunSummary;
use tddy_code_restructuring::spawn_record::{JsonlSpawnRecord, SpawnRecorder};
use tddy_code_restructuring::{Overlay, RestructureError};
use tddy_lsp::{ProcessOutcome, ProcessStart};
use tokio_util::sync::CancellationToken;

/// How long a test lets a run go on before it stops it, when no process ever shows up to stop it by.
const LONG_ENOUGH_FOR_A_PROCESS_TO_START: Duration = Duration::from_secs(20);

fn the_first_argument_of(process: &ProcessStart) -> Option<&str> {
    process.args.first().map(String::as_str)
}

fn programs_among(processes: &[KeptProcess]) -> Vec<&str> {
    processes
        .iter()
        .map(|(start, _)| start.program.as_str())
        .collect()
}

/// The JSON lines a sink wrote, each parsed.
fn the_lines_of(record: &Path) -> Vec<Value> {
    std::fs::read_to_string(record)
        .expect("the record file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a line of the record is JSON"))
        .collect()
}

fn a_record_file_in(directory: &Path) -> PathBuf {
    directory.join("spawns.jsonl")
}

fn a_sink_over(record: &Path) -> JsonlSpawnRecord {
    JsonlSpawnRecord::open(record).expect("the record file opens for appending")
}

fn a_recorder_writing_to(record: &Path) -> SpawnRecorder {
    SpawnRecorder::new(Arc::new(a_sink_over(record)))
}

#[tokio::test(flavor = "multi_thread")]
async fn an_apply_records_every_process_it_starts_with_its_argv_cwd_and_exit_status() {
    // Given an apply over a workspace whose test binary stands alone, recorded by an observer
    let workspace = a_workspace_whose_test_binary_stands_alone();
    let spawns = CollectedSpawns::default();

    // When the move is applied
    let (summary, _) = moving_the_test_binary_recording(
        &workspace,
        false,
        spawns.recorder(),
        CancellationToken::new(),
    )
    .await;

    // Then the apply landed its one operation
    assert_eq!(
        summary,
        Ok(RunSummary {
            applied: 1,
            total: 1,
            stopped_early: false
        })
    );
    // And the record holds what it started: git, cargo (baseline and result) and rustfmt
    let processes = spawns.processes();
    assert!(
        processes.len() >= 5,
        "{} records, expected at least 5: {:?}",
        processes.len(),
        programs_among(&processes)
    );
    let a_git_worktree_probe = processes.iter().any(|(start, _)| {
        start.program == "git" && the_first_argument_of(start) == Some("rev-parse")
    });
    assert!(a_git_worktree_probe, "no `git rev-parse` in {processes:?}");
    let cargo_checks = processes
        .iter()
        .filter(|(start, _)| {
            start.program == "cargo" && the_first_argument_of(start) == Some("check")
        })
        .count();
    assert!(
        cargo_checks >= 2,
        "{cargo_checks} `cargo check`s recorded, expected the baseline and the result"
    );
    assert!(
        programs_among(&processes).contains(&"rustfmt"),
        "no `rustfmt` in {:?}",
        programs_among(&processes)
    );
    // And every process ran in the workspace root and ended with exit status zero
    for (start, outcome) in &processes {
        assert_eq!(start.cwd.as_deref(), Some(workspace.path()), "{start:?}");
        assert!(start.pid.is_some(), "{start:?} has no pid");
        assert_eq!(
            outcome,
            &Some(ProcessOutcome::Exited { code: 0 }),
            "{start:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_baseline_records_the_failing_cargo_check_with_its_exit_status() {
    // Given a workspace whose origin crate does not compile before the plan runs
    let workspace = a_workspace_whose_test_binary_stands_alone();
    workspace.rewriting(ORIGIN_LIB, "pub fn level() -> u32 {\n    \"two\"\n}\n");
    let spawns = CollectedSpawns::default();

    // When the move is applied and refused
    let (summary, _) = moving_the_test_binary_recording(
        &workspace,
        false,
        spawns.recorder(),
        CancellationToken::new(),
    )
    .await;

    // Then it was refused
    assert!(summary.is_err(), "{summary:?}");
    // And the record holds the baseline `cargo check` ending with exit status 101
    let processes = spawns.processes();
    let baseline = processes.iter().find(|(start, _)| {
        start.program == "cargo" && the_first_argument_of(start) == Some("check")
    });
    assert_eq!(
        baseline.map(|(_, outcome)| outcome.clone()),
        Some(Some(ProcessOutcome::Exited { code: 101 })),
        "no failing baseline `cargo check` in {processes:?}"
    );
    // And no process was started to format anything
    assert!(
        !programs_among(&processes).contains(&"rustfmt"),
        "a refused run formatted: {:?}",
        programs_among(&processes)
    );
    // And the refusal still wrote nothing into the tree
    assert!(
        !workspace.holds(".restructure"),
        "recording the refusal created the run's state directory"
    );
}

#[cfg(unix)]
#[test]
fn a_process_killed_by_a_signal_is_recorded_with_the_signal_and_no_exit_code() {
    // Given a recorder over a collecting observer
    let spawns = CollectedSpawns::default();
    let recorder = spawns.recorder();

    // When it runs a shell that kills itself with SIGKILL
    let _ = recorder.output("t", Command::new("sh").args(["-c", "kill -9 $$"]));

    // Then the record says it ended by signal 9, not by an exit code
    let ends: Vec<Option<ProcessOutcome>> = spawns
        .processes()
        .into_iter()
        .map(|(_, outcome)| outcome)
        .collect();
    assert_eq!(ends, vec![Some(ProcessOutcome::Signalled { signal: 9 })]);
}

#[test]
fn a_program_that_cannot_be_started_is_recorded_as_a_failed_spawn() {
    // Given a recorder over a collecting observer
    let spawns = CollectedSpawns::default();
    let recorder = spawns.recorder();

    // When it is asked to run a program that does not exist
    let started = recorder.output("t", &mut Command::new("tddy-no-such-program-anywhere"));

    // Then starting it failed
    assert!(started.is_err());
    // And the record holds one process with no pid, which never started
    let processes = spawns.processes();
    assert_eq!(processes.len(), 1, "{processes:?}");
    let (start, outcome) = &processes[0];
    assert_eq!(start.pid, None);
    assert!(
        matches!(outcome, Some(ProcessOutcome::SpawnFailed { .. })),
        "{outcome:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_compile_gate_records_the_cargo_it_killed_as_signalled() {
    // Given a workspace whose origin crate has a build script that never finishes
    let workspace = a_workspace_whose_origin_build_script_never_finishes();
    let spawns = CollectedSpawns::default();
    let cancel = CancellationToken::new();
    // And a caller who stops waiting once the baseline `cargo check` has started
    let stopper =
        cancelling_once_a_process_has_started(&spawns, &cancel, LONG_ENOUGH_FOR_A_PROCESS_TO_START);

    // When the move is applied
    let (summary, _) =
        moving_the_test_binary_recording(&workspace, false, spawns.recorder(), cancel).await;
    stopper.join().expect("the stopper finishes");

    // Then the run reports that its caller stopped it
    assert_eq!(
        summary.expect_err("a stopped run is not a success"),
        RestructureError::CallerStopped.to_string()
    );
    // And the record shows `cargo` ended by SIGKILL
    let processes = spawns.processes();
    let cargo = processes
        .iter()
        .find(|(start, _)| start.program == "cargo")
        .expect("a `cargo` in the record");
    assert_eq!(cargo.1, Some(ProcessOutcome::Signalled { signal: 9 }));
    // And it names no build script: those are started by cargo, not by anything this run starts
    assert!(
        processes
            .iter()
            .all(|(start, _)| !start.program.contains("build-script")),
        "{:?}",
        programs_among(&processes)
    );
}

#[test]
fn a_record_never_carries_an_environment_value_or_a_secret_looking_argument() {
    // Given a recorder over the JSONL sink
    let directory = tempfile::tempdir().expect("a temporary directory");
    let record = a_record_file_in(directory.path());
    let recorder = a_recorder_writing_to(&record);

    // When it runs `git version` with arguments that carry credentials and an environment value
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "http.extraheader=Authorization: Bearer s3cr3t-header",
            "version",
            "https://deploy:hunter2@host/x",
            "ghp_s3cr3tT0ken",
        ])
        .env("SECRET", "v4lue-never-written");
    recorder
        .output("t", &mut command)
        .expect("git version runs");

    // Then the file holds a placeholder where each credential was
    let written = std::fs::read_to_string(&record).expect("the record file");
    assert!(written.contains("<redacted>"), "{written}");
    // And none of the credentials, nor the environment value
    for secret in [
        "s3cr3t-header",
        "hunter2",
        "ghp_s3cr3tT0ken",
        "v4lue-never-written",
    ] {
        assert!(
            !written.contains(secret),
            "`{secret}` reached the file:\n{written}"
        );
    }
    // And the environment is listed by name
    let start = &the_lines_of(&record)[0];
    assert_eq!(start["env_names"], serde_json::json!(["SECRET"]));
}

#[test]
fn a_program_off_the_allow_list_has_its_arguments_withheld() {
    // Given a recorder over the JSONL sink
    let directory = tempfile::tempdir().expect("a temporary directory");
    let record = a_record_file_in(directory.path());
    let recorder = a_recorder_writing_to(&record);

    // When it runs a program the policy does not know
    recorder
        .output("t", Command::new("echo").args(["alpha", "beta"]))
        .expect("echo runs");

    // Then its argv is the one placeholder, not its arguments
    let start = &the_lines_of(&record)[0];
    assert_eq!(start["program"], "echo");
    assert_eq!(
        start["argv"],
        serde_json::json!(["<2 arguments, not recorded>"])
    );
}

#[test]
fn a_second_run_appends_to_the_record_instead_of_replacing_it() {
    // Given a record file one run has written
    let directory = tempfile::tempdir().expect("a temporary directory");
    let record = a_record_file_in(directory.path());
    a_recorder_writing_to(&record)
        .output("first", Command::new("git").arg("version"))
        .expect("git version runs");
    let first_runs_bytes = std::fs::read(&record).expect("the record file");
    assert!(!first_runs_bytes.is_empty(), "the first run wrote nothing");

    // When a second run opens the same file and starts a process
    a_recorder_writing_to(&record)
        .output("second", Command::new("git").arg("version"))
        .expect("git version runs");

    // Then the first run's lines are still first, byte for byte, and the second run's follow
    let both = std::fs::read(&record).expect("the record file");
    assert!(both.starts_with(&first_runs_bytes));
    assert!(both.len() > first_runs_bytes.len());
}

/// A sink whose file opens and then refuses every write: a named pipe whose only reader has gone.
#[cfg(unix)]
fn a_sink_whose_every_write_fails(directory: &Path) -> JsonlSpawnRecord {
    let pipe = directory.join("record.pipe");
    assert!(Command::new("mkfifo")
        .arg(&pipe)
        .status()
        .expect("mkfifo runs")
        .success());
    // Opening a pipe blocks until its other end is open, so the reader is a thread that goes away
    // as soon as the sink holds the write end.
    let reader = {
        let pipe = pipe.clone();
        std::thread::spawn(move || drop(std::fs::File::open(pipe).expect("the read end opens")))
    };
    let sink = a_sink_over(&pipe);
    reader.join().expect("the reader thread finishes");
    sink
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_record_that_cannot_be_written_does_not_fail_the_run() {
    // Given a record whose every write fails
    let directory = tempfile::tempdir().expect("a temporary directory");
    let sink = a_sink_whose_every_write_fails(directory.path());
    let workspace = a_workspace_whose_test_binary_stands_alone();

    // When the move is applied with it
    let (summary, _) = moving_the_test_binary_recording(
        &workspace,
        false,
        SpawnRecorder::new(Arc::new(sink)),
        CancellationToken::new(),
    )
    .await;

    // Then the apply still lands, as it would without a record
    assert_eq!(
        summary,
        Ok(RunSummary {
            applied: 1,
            total: 1,
            stopped_early: false
        })
    );
}

#[test]
fn a_language_server_the_backend_starts_itself_is_recorded_with_the_names_of_its_pinned_environment(
) {
    // Given a backend that will start the fake language server itself, over a rustup home that
    // names a default toolchain, with a recorder
    let homes = tempfile::tempdir().expect("a temporary directory");
    let (cargo_home, rustup_home) = (homes.path().join("cargo"), homes.path().join("rustup"));
    std::fs::create_dir_all(&cargo_home).expect("a cargo home");
    std::fs::create_dir_all(&rustup_home).expect("a rustup home");
    std::fs::write(
        rustup_home.join("settings.toml"),
        "default_toolchain = \"pinned\"\n",
    )
    .expect("a settings file");
    let spawns = CollectedSpawns::default();
    let workspace = a_workspace_whose_test_binary_stands_alone();
    let mut backend = RustBackend::new(env!("CARGO_BIN_EXE_fake_lsp"), cargo_home, rustup_home)
        .with_spawn_recorder(spawns.recorder());

    // When it is asked to resolve an operation, which starts the server
    let overlay = Overlay::new();
    let an_extraction: tddy_code_restructuring::RefactorOp = serde_json::from_str(&format!(
        r#"{{"op":"extract_method","anchor":{{"kind":"range","file":"{ORIGIN_LIB}","start":{{"line":2,"col":5}},"end":{{"line":2,"col":6}}}},"name":"extracted"}}"#
    ))
    .expect("an extraction parses");
    let _answer = backend.resolve(
        &an_extraction,
        &Workspace {
            root: workspace.path(),
            overlay: &overlay,
        },
    );
    drop(backend);

    // Then the record holds the server's start with the names of the variables pinning it, and no values
    let processes = spawns.processes();
    let (start, outcome) = processes
        .iter()
        .find(|(start, _)| start.purpose == "rust-analyzer")
        .unwrap_or_else(|| panic!("no rust-analyzer start in {processes:?}"));
    assert_eq!(
        start.env_names,
        vec!["CARGO_HOME", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN"]
    );
    // And it has an end, since dropping the backend closes the server's stdin and waits for it
    assert!(outcome.is_some(), "the server's end was not recorded");
}

/// The processes among `processes` that ran `program` with `first` as their first argument.
fn the_starts_of<'a>(
    processes: &'a [KeptProcess],
    program: &str,
    first: &str,
) -> Vec<&'a ProcessStart> {
    processes
        .iter()
        .map(|(start, _)| start)
        .filter(|start| start.program == program && the_first_argument_of(start) == Some(first))
        .collect()
}

/// `#reshape` 10/19: a process started for a plan operation names it, so a spawn record line joins
/// the journal record of the operation that started it.
#[tokio::test(flavor = "multi_thread")]
async fn an_apply_records_the_operation_each_git_process_ran_for() {
    // Given an apply of a test-binary move, recorded by an observer
    let workspace = a_workspace_whose_test_binary_stands_alone();
    let spawns = CollectedSpawns::default();

    // When the move is applied
    let (summary, _) = moving_the_test_binary_recording(
        &workspace,
        false,
        spawns.recorder(),
        CancellationToken::new(),
    )
    .await;

    // Then its `git mv` names operation 0 and the id the run gave it
    assert!(summary.is_ok(), "{summary:?}");
    let processes = spawns.processes();
    let moves = the_starts_of(&processes, "git", "mv");
    assert_eq!(moves.len(), 1, "{:?}", programs_among(&processes));
    let operation = moves[0]
        .operation
        .as_ref()
        .expect("the git mv names its operation");
    assert_eq!(operation.op, Some(0));
    assert!(operation.op_id.is_some(), "{operation:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn run_level_processes_carry_no_operation() {
    // Given an apply of a test-binary move, recorded by an observer
    let workspace = a_workspace_whose_test_binary_stands_alone();
    let spawns = CollectedSpawns::default();

    // When the move is applied
    let _applied = moving_the_test_binary_recording(
        &workspace,
        false,
        spawns.recorder(),
        CancellationToken::new(),
    )
    .await;

    // Then the worktree probe and the baseline and result checks name no operation
    let processes = spawns.processes();
    let run_level: Vec<&ProcessStart> = the_starts_of(&processes, "git", "rev-parse")
        .into_iter()
        .chain(the_starts_of(&processes, "cargo", "check"))
        .collect();
    assert!(run_level.len() >= 3, "{:?}", programs_among(&processes));
    assert!(
        run_level.iter().all(|start| start.operation.is_none()),
        "{run_level:#?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_groups_compile_gate_is_recorded_with_its_group() {
    // Given a test-binary move that is the one member of the group `g`, recorded by an observer
    let workspace = a_workspace_whose_test_binary_stands_alone();
    let spawns = CollectedSpawns::default();

    // When the move is applied
    let summary =
        moving_the_test_binary_in_a_group_recording(&workspace, "g", spawns.recorder()).await;

    // Then one `cargo check` — the group's end-of-group gate — names the group
    assert!(summary.is_ok(), "{summary:?}");
    let processes = spawns.processes();
    let gated: Vec<&ProcessStart> = the_starts_of(&processes, "cargo", "check")
        .into_iter()
        .filter(|start| {
            start
                .operation
                .as_ref()
                .is_some_and(|operation| operation.group.as_deref() == Some("g"))
        })
        .collect();
    assert_eq!(
        gated.len(),
        1,
        "{:#?}",
        the_starts_of(&processes, "cargo", "check")
    );
}

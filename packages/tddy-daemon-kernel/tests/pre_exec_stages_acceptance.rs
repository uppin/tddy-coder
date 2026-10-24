//! Acceptance: a child started as a target account reports each pre-exec step to the parent, the
//! parent logs it with the child's pid, and a step that does not finish is logged again until it
//! does — so a hung spawn says where it hung.
//!
//! A real child is spawned as the **current** user (no privilege change, so no root needed). A
//! child held mid-`pre_exec` cannot be produced without a test-only hook in production code, so the
//! heartbeat is pinned on the watcher itself, fed by a pipe this test writes.
//!
//! Feature docs: docs/ft/daemon/project-concept.md (Cloning at session start)
#![cfg(unix)]

mod support;

use std::io::Write;
use std::os::fd::FromRawFd;
use std::process::{Command, Stdio};
use std::time::Duration;

use support::captured_log::{captured_log, everything, logged};
use tddy_daemon_kernel::pre_exec_stages::{
    stage_record, watch_stages, PreExecStage, StageOutcome, StagedCommand, STAGE_RECORD_LEN,
};
use tddy_daemon_kernel::privilege_drop::PrivilegeChange;

fn a_pipe() -> (std::fs::File, std::fs::File) {
    let mut fds = [0; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0, "pipe(2)");
    unsafe {
        (
            std::fs::File::from_raw_fd(fds[0]),
            std::fs::File::from_raw_fd(fds[1]),
        )
    }
}

const EVERY_STAGE: [PreExecStage; 5] = [
    PreExecStage::Forked,
    PreExecStage::Groups,
    PreExecStage::Gid,
    PreExecStage::Uid,
    PreExecStage::Exec,
];

#[test]
fn a_same_user_child_reports_forked_then_exec_and_runs() {
    // Given
    let log = captured_log();
    let mut echo = Command::new("/bin/echo");
    echo.arg("staged child ran").stdout(Stdio::piped());

    // When
    let staged = StagedCommand::new(echo, "echo acceptance", PrivilegeChange::None)
        .spawn()
        .expect("a same-user child spawns");
    let pid = staged.child.id().to_string();
    let output = staged.child.wait_with_output().unwrap();
    let outcome = staged.stages.join().unwrap();

    // Then — the program ran, and the log followed it to exec
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "staged child ran"
    );
    assert_eq!(outcome, StageOutcome::Execed, "the last record was exec");
    let pid_field = format!("pid={pid}");
    assert!(
        logged(
            log,
            log::Level::Info,
            &["spawn echo acceptance", &pid_field, "reached forked"]
        ) && logged(
            log,
            log::Level::Info,
            &["spawn echo acceptance", &pid_field, "reached exec"]
        ),
        "both steps logged with the child's pid:\n{}",
        everything(log)
    );
}

#[test]
fn a_child_held_at_a_stage_is_logged_as_still_there() {
    // Given — a child that reported it forked and then went quiet
    let log = captured_log();
    let (reader, mut writer) = a_pipe();
    writer
        .write_all(&stage_record(PreExecStage::Forked, 4242))
        .unwrap();
    let watcher = std::thread::spawn(move || {
        watch_stages(
            reader,
            "clone /home/mantas/repos/tddy-coder",
            Duration::from_millis(40),
        )
    });

    // When — several heartbeats pass, then the pipe closes without exec
    std::thread::sleep(Duration::from_millis(200));
    drop(writer);
    let outcome = watcher.join().unwrap();

    // Then
    assert_eq!(outcome, StageOutcome::StoppedAt(PreExecStage::Forked));
    assert!(
        logged(
            log,
            log::Level::Warn,
            &[
                "spawn clone /home/mantas/repos/tddy-coder",
                "pid=4242",
                "still at forked after"
            ]
        ),
        "an outstanding step is logged again as a warning:\n{}",
        everything(log)
    );
}

#[test]
fn a_pipe_that_closes_before_any_record_reports_nothing() {
    // Given
    let (reader, writer) = a_pipe();
    drop(writer);

    // When / Then
    assert_eq!(
        watch_stages(reader, "nothing", Duration::from_millis(40)),
        StageOutcome::NothingReported
    );
}

#[test]
fn every_stage_survives_its_record() {
    for stage in EVERY_STAGE {
        // When
        let record = stage_record(stage, 0x0102_0304);

        // Then — one byte of step, then the pid little-endian
        assert_eq!(record.len(), STAGE_RECORD_LEN);
        assert_eq!(PreExecStage::from_byte(record[0]), Some(stage), "{stage:?}");
        assert_eq!(
            &record[1..],
            &0x0102_0304u32.to_le_bytes(),
            "{stage:?} pid bytes"
        );
    }
}

#[test]
fn stages_log_under_their_own_names() {
    assert_eq!(
        EVERY_STAGE.map(PreExecStage::name),
        ["forked", "groups", "gid", "uid", "exec"]
    );
}

#[test]
fn a_byte_that_names_no_stage_is_rejected() {
    assert_eq!(PreExecStage::from_byte(0), None);
    assert_eq!(PreExecStage::from_byte(6), None);
}

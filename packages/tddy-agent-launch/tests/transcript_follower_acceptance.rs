//! Acceptance: the daemon follows an agent's own transcript file — on this host, or on the
//! session's target (SSH or Coder, the agent in tmux or not) — and delivers what each line says,
//! mapped by the agent's spec, to the session's activity handling.
//!
//! - **Path.** On a target the transcript is the target's: its path is rendered with the target
//!   shell's `$HOME` and the target's worktree, never this host's.
//! - **Pty.** `coder ssh` (and an ssh that allocates one) runs the follower's `tail` in a pty; the
//!   offset still counts the file's bytes, so a reopened stream starts where delivery stopped.
//! - **tmux.** An agent in tmux outlives its follower — a dropped connection, a re-attach, a
//!   restarted daemon. A follower started again with the session's persisted offset reads on from
//!   the last line delivered while the agent kept writing: nothing lost, nothing twice.
//!
//! The fixture is the real Cursor probe transcript (`tddy-coding-agent-specs`'
//! `tests/fixtures/cursor-agent-transcript.jsonl`), mapped by the real `cursor.yaml`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use tddy_agent_launch::log_follower::{tail_command, FileFollower, FollowOffset};
use tddy_agent_launch::remote_hook_follower::HookReportDelivery;
use tddy_agent_launch::transcript_follower::{
    follow_transcript, home_on, transcript_offset, transcript_path, TranscriptOwner,
};
use tddy_coding_agent_specs::{TranscriptEntry, TranscriptSource};
use tddy_remote_shell::{ExecOutput, InMemoryShell, LocalShell, ShellCall};
use tddy_service::proto::activity::{
    AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse, ReportAgentActivityRequest,
    ReportSessionStatusRequest, ReportSessionStatusResponse,
};
use tddy_session_activity::TranscriptLine;
use tddy_testing_commons::wait::eventually;

const SESSION_ID: &str = "01a20f05-97d8-7221-85bc-d879ececfb62";
const CHAT_ID: &str = "6a42ca72-f90d-4d19-9b8e-618ece0956b5";
const TARGET_WORKTREE: &str =
    "/home/coder/.tddy/repos/qape-hq/.worktrees/01a20f05-97d8-7221-85bc-d879ececfb62";
const TARGET_HOOK_LOG: &str =
    "/home/coder/.tddy/repos/qape-hq/.worktrees/01a20f05-97d8-7221-85bc-d879ececfb62/.tddy/hooks.log";
/// Where Cursor on the target writes the session's transcript.
const TARGET_TRANSCRIPT: &str = "/home/coder/.cursor/projects/\
    home-coder-tddy-repos-qape-hq-worktrees-01a20f05-97d8-7221-85bc-d879ececfb62/\
    agent-transcripts/6a42ca72-f90d-4d19-9b8e-618ece0956b5/6a42ca72-f90d-4d19-9b8e-618ece0956b5.jsonl";
/// A safety net, not a prediction.
const OBSERVE: Duration = Duration::from_secs(30);

fn cursor_transcript() -> TranscriptSource {
    tddy_coding_agent_specs::embedded_spec("cursor")
        .expect("cursor is built in")
        .activity()
        .transcript
        .clone()
        .expect("cursor's spec declares its transcript")
}

/// The probe's transcript, each line with its newline.
fn the_probe_lines() -> Vec<String> {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tddy-coding-agent-specs/tests/fixtures/cursor-agent-transcript.jsonl"),
    )
    .expect("the fixture")
    .lines()
    .map(|line| format!("{line}\n"))
    .collect()
}

/// What the probe's lines say, line by line, as the spec maps them.
fn mapped(lines: &[String]) -> Vec<Vec<TranscriptEntry>> {
    let transcript = cursor_transcript();
    lines
        .iter()
        .map(|line| transcript.map_line(&serde_json::from_str(line.trim_end()).unwrap()))
        .collect()
}

/// The daemon's end: every transcript line delivered, in order.
#[derive(Default)]
struct TheDaemonsActivity {
    lines: Mutex<Vec<TranscriptLine>>,
}

#[async_trait]
impl HookReportDelivery for TheDaemonsActivity {
    async fn report_session_status(
        &self,
        _: ReportSessionStatusRequest,
    ) -> Result<ReportSessionStatusResponse, String> {
        Err("no hook is delivered here".to_string())
    }

    async fn await_question_answer(
        &self,
        _: AwaitQuestionAnswerRequest,
    ) -> Result<AwaitQuestionAnswerResponse, String> {
        Err("no hook is delivered here".to_string())
    }

    async fn report_agent_activity(&self, _: ReportAgentActivityRequest) -> Result<(), String> {
        Err("no hook is delivered here".to_string())
    }

    async fn record_transcript_line(&self, line: TranscriptLine) -> Result<(), String> {
        self.lines.lock().unwrap().push(line);
        Ok(())
    }
}

impl TheDaemonsActivity {
    /// The entries of each line delivered, once `count` lines have been.
    async fn once_it_holds(&self, count: usize) -> Vec<Vec<TranscriptEntry>> {
        eventually(&format!("{count} transcript lines"), OBSERVE, || {
            let lines = self.lines.lock().unwrap().clone();
            (lines.len() >= count)
                .then(|| lines.iter().map(|line| line.entries.clone()).collect())
                .ok_or(format!("delivered so far: {}", lines.len()))
        })
        .await
    }

    fn end_offsets(&self) -> Vec<u64> {
        self.lines
            .lock()
            .unwrap()
            .iter()
            .map(|line| line.end_offset)
            .collect()
    }
}

/// A session's directory holding the usage context a launch records.
fn a_session_dir(worktree: &str, agent_home: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a session dir");
    tddy_usage::write_usage_context(
        dir.path(),
        &tddy_usage::UsageContextFile {
            spec_id: "cursor".to_string(),
            // What a target start records today: this host's home, which is not the target's.
            agent_home: Some(PathBuf::from(agent_home)),
            conversation_id: Some(CHAT_ID.to_string()),
            worktree: Some(PathBuf::from(worktree)),
            model: Some("auto".to_string()),
        },
    )
    .expect("the usage context");
    dir
}

fn owner() -> TranscriptOwner {
    TranscriptOwner {
        session_id: SESSION_ID.to_string(),
        os_user: "dev".to_string(),
    }
}

/// A Coder target whose shell's `$HOME` is `/home/coder`.
fn a_coder_target() -> InMemoryShell {
    let shell = InMemoryShell::new("coder:dev/remote-dev");
    shell.answers(
        "printf '%s' \"$HOME\"",
        ExecOutput::succeeded("/home/coder"),
    );
    shell
}

#[tokio::test]
async fn on_a_target_the_transcript_is_found_under_the_targets_home_not_this_hosts() {
    // Given a target session whose launch recorded this host's home
    let target = a_coder_target();
    let session = a_session_dir(TARGET_WORKTREE, "/Users/dev");

    // When the follower asks the target where its home is and places the transcript
    let home = home_on(&target).await.expect("the target's home");
    let path = transcript_path(&cursor_transcript(), session.path(), Some(home));

    // Then
    assert_eq!(path, Ok(TARGET_TRANSCRIPT.to_string()));
}

#[tokio::test(start_paused = true)]
async fn behind_a_coder_pty_the_transcripts_lines_arrive_and_a_reopened_tail_starts_at_the_files_byte(
) {
    // Given a Coder target whose pty turns every `\n` into `\r\n`
    let target = a_coder_target();
    let lines = the_probe_lines();
    let first_two = lines[0].len() + lines[1].len();
    let tail = target.streams(tail_command(TARGET_TRANSCRIPT, 0, TARGET_HOOK_LOG, true));
    let resumed = target.streams(tail_command(
        TARGET_TRANSCRIPT,
        first_two as u64,
        TARGET_HOOK_LOG,
        true,
    ));
    let daemon = Arc::new(TheDaemonsActivity::default());
    let _follower = follow_transcript(
        Arc::new(target.clone()),
        (TARGET_TRANSCRIPT, TARGET_HOOK_LOG),
        cursor_transcript(),
        owner(),
        daemon.clone(),
        FollowOffset::in_memory(),
    );

    // When two lines arrive, the connection drops, and the rest arrive on the reopened tail
    tail.prints(lines[..2].concat().replace('\n', "\r\n")).await;
    daemon.once_it_holds(2).await;
    tail.ends_with(255).await;
    resumed
        .prints(lines[2..].concat().replace('\n', "\r\n"))
        .await;
    let delivered = daemon.once_it_holds(lines.len()).await;

    // Then every line is delivered once, as the spec maps it, and the reopened tail started at the
    // file's byte, not the stream's
    let streamed: Vec<String> = target
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            ShellCall::ExecStreamed(command) => Some(command),
            _ => None,
        })
        .collect();
    assert_eq!(
        (delivered, streamed),
        (
            mapped(&lines),
            vec![
                tail_command(TARGET_TRANSCRIPT, 0, TARGET_HOOK_LOG, true),
                tail_command(TARGET_TRANSCRIPT, first_two as u64, TARGET_HOOK_LOG, true),
            ]
        )
    );
}

/// The machine an agent runs on, as a real directory read through a real `/bin/sh`: its
/// transcript, the file keeping the follower's `tail` alive, and the session's directory.
struct AMachine {
    dir: tempfile::TempDir,
}

impl AMachine {
    fn new() -> Self {
        let machine = Self {
            dir: tempfile::tempdir().expect("a machine"),
        };
        std::fs::write(machine.alive(), b"").expect("the lease");
        std::fs::create_dir_all(machine.session_dir()).expect("the session dir");
        machine
    }

    fn transcript(&self) -> PathBuf {
        self.dir.path().join("agent-transcripts/c/c.jsonl")
    }

    fn alive(&self) -> PathBuf {
        self.dir.path().join("alive")
    }

    fn session_dir(&self) -> PathBuf {
        self.dir.path().join("session")
    }

    /// The agent appends `lines` to its transcript, creating it with its first.
    fn agent_writes(&self, lines: &[String]) {
        use std::io::Write as _;
        std::fs::create_dir_all(self.transcript().parent().unwrap()).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.transcript())
            .expect("the transcript");
        file.write_all(lines.concat().as_bytes()).unwrap();
    }

    /// A follower of the transcript through this machine's shell, from the session's persisted
    /// offset, as a launch, a resume or a re-attach starts one.
    fn followed_into(&self, daemon: &Arc<TheDaemonsActivity>) -> FileFollower {
        follow_transcript(
            Arc::new(LocalShell::new()),
            (
                &self.transcript().to_string_lossy(),
                &self.alive().to_string_lossy(),
            ),
            cursor_transcript(),
            owner(),
            daemon.clone(),
            transcript_offset(&self.session_dir()),
        )
    }
}

impl Drop for AMachine {
    fn drop(&mut self) {
        // The `tail` ends once its lease is gone.
        let _ = std::fs::remove_file(self.alive());
    }
}

#[tokio::test]
async fn a_host_sessions_transcript_is_followed_from_before_it_exists() {
    // Given a session on this host whose agent has not been prompted yet: no transcript exists
    let machine = AMachine::new();
    let daemon = Arc::new(TheDaemonsActivity::default());
    let lines = the_probe_lines();
    let _follower = machine.followed_into(&daemon);

    // When the agent writes its transcript
    machine.agent_writes(&lines);
    let delivered = daemon.once_it_holds(lines.len()).await;

    // Then every line is delivered, in order, as the spec maps it
    assert_eq!(delivered, mapped(&lines));
}

#[tokio::test]
async fn an_agent_in_tmux_keeps_writing_while_its_follower_restarts_and_nothing_is_lost_or_doubled()
{
    // Given an agent whose first two lines its follower delivered
    let machine = AMachine::new();
    let daemon = Arc::new(TheDaemonsActivity::default());
    let lines = the_probe_lines();
    let first = machine.followed_into(&daemon);
    machine.agent_writes(&lines[..2]);
    daemon.once_it_holds(2).await;

    // When the follower stops — a dropped connection, a restarted daemon — the agent, in tmux,
    // writes on, and a follower is started again for the session
    first.stop().await;
    machine.agent_writes(&lines[2..]);
    let _second = machine.followed_into(&daemon);
    let delivered = daemon.once_it_holds(lines.len()).await;

    // Then each line is delivered exactly once, in order, each at its own end in the file
    let ends: Vec<u64> = lines
        .iter()
        .scan(0u64, |end, line| {
            *end += line.len() as u64;
            Some(*end)
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!((delivered, daemon.end_offsets()), (mapped(&lines), ends));
}

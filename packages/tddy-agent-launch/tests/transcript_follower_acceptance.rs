//! Acceptance: the daemon follows an agent's own transcript file — on this host, or on the
//! session's target (SSH or Coder, the agent in tmux or not) — and delivers what each line says,
//! mapped by the agent's spec, to the session's activity handling.
//!
//! - **Path.** On a target the transcript is the target's: its path is rendered with the target
//!   shell's `$HOME` and the target's worktree, never this host's.
//! - **Rewrite.** Cursor rewrites its transcript atomically on every new prompt; polling reads
//!   only the new suffix, so a new prompt, a reattach and a resume each append only new lines.
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
use tddy_agent_launch::log_follower::FileFollower;
use tddy_agent_launch::remote_hook_follower::{HookDeliveryError, HookReportDelivery};
use tddy_agent_launch::transcript_follower::{
    follow_transcript, home_on, transcript_offset, transcript_path, TranscriptOwner,
};
use tddy_coding_agent_specs::{TranscriptEntry, TranscriptSource};
use tddy_remote_shell::{ExecOutput, InMemoryShell, LocalShell};
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
    ) -> Result<ReportSessionStatusResponse, HookDeliveryError> {
        Err(HookDeliveryError::failed_precondition(
            "no hook is delivered here",
        ))
    }

    async fn await_question_answer(
        &self,
        _: AwaitQuestionAnswerRequest,
    ) -> Result<AwaitQuestionAnswerResponse, HookDeliveryError> {
        Err(HookDeliveryError::failed_precondition(
            "no hook is delivered here",
        ))
    }

    async fn report_agent_activity(
        &self,
        _: ReportAgentActivityRequest,
    ) -> Result<(), HookDeliveryError> {
        Err(HookDeliveryError::failed_precondition(
            "no hook is delivered here",
        ))
    }

    async fn record_transcript_line(&self, line: TranscriptLine) -> Result<(), HookDeliveryError> {
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

    /// The agent rewrites its transcript atomically with `lines` (a temp file renamed over it),
    /// as Cursor does on every new prompt: the old prefix stable, new lines appended.
    fn agent_rewrites(&self, lines: &[String]) {
        std::fs::create_dir_all(self.transcript().parent().unwrap()).unwrap();
        let tmp = self.transcript().with_extension("jsonl.tmp");
        std::fs::write(&tmp, lines.concat()).expect("the temp transcript");
        std::fs::rename(&tmp, self.transcript()).expect("the atomic replace");
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
    // Two polls past delivery: a replay would have arrived by now.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!((delivered, daemon.end_offsets()), (mapped(&lines), ends));
}

#[tokio::test]
async fn a_new_prompt_a_reattach_and_a_resume_each_append_only_new_lines() {
    // Given an agent whose first two lines its follower delivered (n2: every new prompt re-read
    // Cursor's JSONL from byte 0, replaying turn 1 three times and turn 2 twice)
    let machine = AMachine::new();
    let daemon = Arc::new(TheDaemonsActivity::default());
    let lines = the_probe_lines();
    let first = machine.followed_into(&daemon);
    machine.agent_writes(&lines[..2]);
    daemon.once_it_holds(2).await;

    // When a new prompt makes Cursor rewrite its transcript atomically with two more lines —
    // the old prefix stable, as the probe's file holds it
    machine.agent_rewrites(&lines[..4]);
    let delivered = daemon.once_it_holds(4).await;

    // Then only the new lines arrive, each once: no replay of the first two
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(
        (delivered, daemon.end_offsets().len()),
        (mapped(&lines[..4]), 4),
        "a new prompt replays old lines"
    );

    // When the follower is reattached (same offset, new reader) with no new lines, nothing arrives
    first.stop().await;
    let reattached = machine.followed_into(&daemon);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(
        daemon.end_offsets().len(),
        4,
        "a reattach replays old lines"
    );

    // When a resume appends one more line and restarts the follower, only it arrives
    // (the probe file has four lines; a fifth stands in for the next turn's prompt).
    reattached.stop().await;
    let extra = "{\"role\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"prompt five\"}]}}\n"
        .to_string();
    let mut five = lines[..4].to_vec();
    five.push(extra);
    machine.agent_rewrites(&five);
    let _resumed = machine.followed_into(&daemon);
    let delivered = daemon.once_it_holds(5).await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(
        (delivered.len(), daemon.end_offsets().len()),
        (5, 5),
        "a resume replays old lines"
    );
}

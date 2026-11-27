use crate::test_util::TestDaemon;
use std::path::PathBuf;
use tddy_fs::Fs as _;

impl TestDaemon {
    /// Make `session_id` live as the daemon defines it: hold a main agent terminal for it. The
    /// agent is a stub that records the first `INT` or `TERM` it receives, then exits; it is ready
    /// (its traps installed) when this returns. A `pid` recorded in `.session.yaml` makes a
    /// session active no longer; this does.
    pub async fn holds_a_main_agent(&self, session_id: &str) -> HeldMain {
        let dir = tempfile::tempdir().expect("a directory for the stub agent");
        let record = dir.path().join("signal-received");
        let ready = dir.path().join("ready");
        let script = dir.path().join("agent.sh");
        // `DiskFs`: a fixture helper, not production code — but the port guard reads this file
        // textually, and the stub script must be a real executable file either way.
        tddy_fs::DiskFs.write(
            &script,
            format!(
                "#!/bin/sh\ntrap 'echo TERM > \"{r}\"; exit 0' TERM\ntrap 'echo INT > \"{r}\"; exit 0' INT\ntouch \"{ready}\"\nwhile :; do sleep 0.05; done\n",
                r = record.display(),
                ready = ready.display()
            )
            .as_bytes(),
        )
        .expect("write the stub agent");
        tddy_fs::DiskFs
            .set_mode(&script, 0o755)
            .expect("make the stub agent executable");
        let pid = self
            .inner
            .terminal_manager()
            .start_with_options(
                session_id,
                std::env::temp_dir(),
                "test-model",
                vec![script.to_string_lossy().to_string()],
                "claude-cli",
                None,
                None,
                Vec::new(),
                Vec::new(),
                None,
            )
            .await
            .expect("a main agent terminal starts")
            .pid;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !ready.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "the stub agent never installed its traps"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        HeldMain {
            pid,
            record,
            _dir: dir,
        }
    }

    /// Whether the daemon still serves `session_id`'s main terminal.
    pub async fn serves_a_main_for(&self, session_id: &str) -> bool {
        self.inner
            .terminal_manager()
            .get(session_id)
            .await
            .is_some()
    }

    /// Stop everything the daemon holds for `session_id`: the session is no longer live.
    pub async fn stops_holding(&self, session_id: &str) {
        self.inner.terminal_manager().stop_session(session_id).await;
    }
}

/// A main agent a [`TestDaemon`] holds for a session ([`TestDaemon::holds_a_main_agent`]).
pub struct HeldMain {
    /// The agent's process id: where a signal to its session lands.
    pub pid: u32,
    record: PathBuf,
    _dir: tempfile::TempDir,
}

impl HeldMain {
    /// The signal (`INT` or `TERM`) the agent received, waiting up to ten seconds for it.
    pub async fn signal_received(&self) -> Option<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Ok(text) = tddy_fs::DiskFs.read_to_string(&self.record) {
                return Some(text.trim().to_string());
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}

/// Open a conversation of `session_id` with `agent_id` whose turn loop runs on
/// `daemon_instance_id`, as the subagent route records a forwarded open — so a test can see what a
/// close or a delete of the session does with it.
pub async fn open_a_remote_conversation(
    host: &crate::connection_service::DaemonSessionHost,
    conversation_id: &str,
    session_id: &str,
    agent_id: &str,
    daemon_instance_id: &str,
) {
    host.open_agent_conversations()
        .insert(
            conversation_id.to_string(),
            tddy_session_agents::AgentConversation::Remote {
                session_id: session_id.to_string(),
                agent_id: agent_id.to_string(),
                daemon_instance_id: daemon_instance_id.to_string(),
            },
        )
        .await;
}

/// Whether `conversation_id` is still open on `host`.
pub async fn conversation_is_open(
    host: &crate::connection_service::DaemonSessionHost,
    conversation_id: &str,
) -> bool {
    host.open_agent_conversations()
        .contains(conversation_id)
        .await
}

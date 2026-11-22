//! What a stub agent recorded about each of its runs — shared by the target-session suites
//! (`coder_target_session_acceptance`, `remote_target_session_acceptance`), whose stub agents append
//! one tab-separated line per run: its pid, its working directory, then its arguments.

use std::path::{Path, PathBuf};

/// One run of the stub agent, as it recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnAgentRun {
    pub pid: i32,
    pub cwd: PathBuf,
    pub args: Vec<String>,
}

impl AnAgentRun {
    pub fn is_a_resume(&self) -> bool {
        self.args.iter().any(|arg| arg == "--resume")
    }

    pub fn is_alive(&self) -> bool {
        // SAFETY: signal 0 only probes whether the pid still exists.
        unsafe { libc::kill(self.pid, 0) == 0 }
    }
}

/// Every run recorded in the file `runs` so far, oldest first; none while the file is absent.
pub fn read_agent_runs(runs: &Path) -> Vec<AnAgentRun> {
    std::fs::read_to_string(runs)
        .unwrap_or_default()
        .lines()
        .map(|line| line.split('\t').map(str::to_string).collect::<Vec<_>>())
        .filter(|fields| fields.len() >= 2)
        .map(|fields| AnAgentRun {
            pid: fields[0].parse().expect("a run records its pid"),
            cwd: PathBuf::from(&fields[1]),
            args: fields[2..].to_vec(),
        })
        .collect()
}

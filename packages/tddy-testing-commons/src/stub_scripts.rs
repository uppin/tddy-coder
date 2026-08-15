//! Shell stubs that stand in for a real agent binary, and the readers that observe them.
//!
//! Tests that assert on the argv a spawned agent received need a fake binary that records what it
//! was called with. The recording is where these tests go wrong: a stub that appends its argv one
//! `printf` at a time lets a reader observe a *half-written* line, and the resulting failure looks
//! exactly like the daemon having built the wrong argv. Raising the reader's timeout does not fix
//! that — the record was torn, not late.
//!
//! So every recording here is a **single** write:
//!
//! - [`StubAgentScript::recording_argv_to`] writes a temp file and `mv -f`s it over the target, so
//!   a reader sees either the previous invocation's argv or this one's, never a mixture.
//! - [`StubAgentScript::recording_env_to`] does the same for named environment variables, for tests
//!   that assert on how a spawned agent was wired rather than on what it was called with.
//! - [`StubAgentScript::appending_argv_to`] joins the whole line in a shell variable and appends it
//!   with one `printf`, which `O_APPEND` makes atomic for line-sized writes.
//!
//! The readers are probe-shaped (`Result<T, String>`) so they drop straight into
//! [`crate::wait::eventually_blocking`] / [`crate::wait::eventually`], which turn a missing record
//! into a message naming what was actually on disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Builder for a `/bin/sh` stub. Sections run in the order they are declared on the builder.
#[derive(Debug)]
pub struct StubAgentScript {
    path: PathBuf,
    body: String,
}

/// Starts a stub script at `dir/file_name`. Nothing is written until [`StubAgentScript::build`].
pub fn a_stub_agent_script(dir: &Path, file_name: &str) -> StubAgentScript {
    StubAgentScript {
        path: dir.join(file_name),
        body: String::from("#!/bin/sh\n"),
    }
}

impl StubAgentScript {
    /// Raw `sh` to run before anything else — for a stub that must answer a subcommand (say
    /// `create-chat`) before it behaves like a normal agent launch.
    pub fn with_prelude(mut self, sh: &str) -> Self {
        self.body.push_str(sh.trim_end());
        self.body.push('\n');
        self
    }

    /// Echo the argv to stdout as `ARGV: …`, for tests that read it back off the PTY.
    pub fn echoing_argv(mut self) -> Self {
        self.body.push_str("echo \"ARGV: $@\"\n");
        self
    }

    /// Echo `NAME=value` to stdout for each named variable, for tests asserting on the child env.
    pub fn echoing_env(mut self, names: &[&str]) -> Self {
        for name in names {
            self.body
                .push_str(&format!("echo \"{name}=${{{name}}}\"\n"));
        }
        self
    }

    /// Record this invocation's argv, one argument per line, replacing any previous record.
    ///
    /// Written via temp file + `mv -f`: [`read_recorded_argv`] can never observe a partial record.
    pub fn recording_argv_to(mut self, argv_file: &Path) -> Self {
        let target = argv_file.display();
        self.body.push_str(&format!(
            "printf '%s\\n' \"$@\" > \"{target}.tmp.$$\"\nmv -f \"{target}.tmp.$$\" \"{target}\"\n"
        ));
        self
    }

    /// Record the named environment variables this invocation was launched with, one `NAME=value`
    /// line each, replacing any previous record.
    ///
    /// Written via temp file + `mv -f`, like [`Self::recording_argv_to`]: [`read_recorded_env`] can
    /// never observe a partial record. A variable the spawn never set is recorded as `NAME=`, so a
    /// missing wiring reads as an empty value rather than as a record that has not landed yet.
    pub fn recording_env_to(mut self, env_file: &Path, names: &[&str]) -> Self {
        let target = env_file.display();
        self.body.push_str(&format!(": > \"{target}.tmp.$$\"\n"));
        for name in names {
            self.body.push_str(&format!(
                "printf '%s=%s\\n' \"{name}\" \"${{{name}}}\" >> \"{target}.tmp.$$\"\n"
            ));
        }
        self.body
            .push_str(&format!("mv -f \"{target}.tmp.$$\" \"{target}\"\n"));
        self
    }

    /// Append this invocation's argv to a log as one tab-separated line, keeping earlier lines.
    ///
    /// The line is assembled in a variable and appended with a single `printf`, so a concurrent
    /// reader sees whole invocations only — see the module docs.
    pub fn appending_argv_to(mut self, argv_log: &Path) -> Self {
        let target = argv_log.display();
        self.body.push_str(&format!(
            r#"TDDY_STUB_TAB=$(printf '\t')
TDDY_STUB_LINE=
TDDY_STUB_FIRST=1
for tddy_stub_arg in "$@"; do
  if [ "$TDDY_STUB_FIRST" = 1 ]; then
    TDDY_STUB_LINE=$tddy_stub_arg
    TDDY_STUB_FIRST=0
  else
    TDDY_STUB_LINE=$TDDY_STUB_LINE$TDDY_STUB_TAB$tddy_stub_arg
  fi
done
printf '%s\n' "$TDDY_STUB_LINE" >> "{target}"
"#
        ));
        self
    }

    /// Stay alive reading stdin, standing in for an agent that holds its PTY open.
    pub fn then_reading_stdin(mut self) -> Self {
        self.body.push_str("exec cat\n");
        self
    }

    /// Stay alive for `secs` — long enough to outlive a caller's startup grace period.
    pub fn then_sleeping_secs(mut self, secs: u32) -> Self {
        self.body.push_str(&format!("sleep {secs}\n"));
        self
    }

    /// Writes the script and marks it executable, returning its path.
    pub fn build(self) -> PathBuf {
        std::fs::write(&self.path, &self.body).unwrap_or_else(|e| {
            panic!("write stub script {}: {e}", self.path.display());
        });
        make_executable(&self.path);
        self.path
    }
}

/// Marks `path` executable by everyone (`0o755`).
pub fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|e| panic!("chmod +x {}: {e}", path.display()));
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Reads the argv recorded by [`StubAgentScript::recording_argv_to`].
///
/// Probe-shaped for [`crate::wait::eventually_blocking`]: `Err` describes what is on disk now.
pub fn read_recorded_argv(argv_file: &Path) -> Result<Vec<String>, String> {
    let contents = std::fs::read_to_string(argv_file)
        .map_err(|e| format!("{} is not readable yet: {e}", argv_file.display()))?;
    let argv: Vec<String> = contents.lines().map(str::to_string).collect();
    if argv.is_empty() {
        return Err(format!("{} exists but is empty", argv_file.display()));
    }
    Ok(argv)
}

/// The value immediately following `flag` in a recorded argv, e.g. `--tddy-data-dir`'s path.
///
/// Probe-shaped: `Err` reports the argv actually recorded, which is what a caller needs to see
/// when the daemon built a different command line than expected.
pub fn read_recorded_argv_value_after(argv_file: &Path, flag: &str) -> Result<String, String> {
    let argv = read_recorded_argv(argv_file)?;
    argv.iter()
        .position(|a| a == flag)
        .and_then(|i| argv.get(i + 1))
        .map(String::from)
        .ok_or_else(|| format!("recorded argv has no {flag}: {argv:?}"))
}

/// Reads the environment recorded by [`StubAgentScript::recording_env_to`].
///
/// Probe-shaped for [`crate::wait::eventually_blocking`]: `Err` describes what is on disk now.
pub fn read_recorded_env(env_file: &Path) -> Result<HashMap<String, String>, String> {
    let contents = std::fs::read_to_string(env_file)
        .map_err(|e| format!("{} is not readable yet: {e}", env_file.display()))?;
    let mut recorded = HashMap::new();
    for line in contents.lines().filter(|l| !l.is_empty()) {
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| format!("{line:?} is not a NAME=value line"))?;
        recorded.insert(name.to_string(), value.to_string());
    }
    if recorded.is_empty() {
        return Err(format!("{} exists but is empty", env_file.display()));
    }
    Ok(recorded)
}

/// The most recent line appended by [`StubAgentScript::appending_argv_to`], split on tabs.
///
/// Probe-shaped for [`crate::wait::eventually`].
pub fn read_last_appended_argv(argv_log: &Path) -> Result<Vec<String>, String> {
    let contents = std::fs::read_to_string(argv_log)
        .map_err(|e| format!("{} is not readable yet: {e}", argv_log.display()))?;
    let last = contents
        .lines()
        .rfind(|l| !l.is_empty())
        .ok_or_else(|| format!("{} recorded no invocation yet", argv_log.display()))?;
    Ok(last.split('\t').map(str::to_string).collect())
}

/// How many invocations [`StubAgentScript::appending_argv_to`] has logged.
pub fn appended_invocation_count(argv_log: &Path) -> usize {
    std::fs::read_to_string(argv_log)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;

    fn a_temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp dir")
    }

    fn run(script: &Path, args: &[&str]) {
        let status = Command::new(script)
            .args(args)
            .status()
            .expect("run stub script");
        assert!(status.success(), "stub script exited {status}");
    }

    #[test]
    fn a_recording_stub_reports_the_argv_it_was_invoked_with() {
        // Given
        let dir = a_temp_dir();
        let argv_file = dir.path().join("argv.txt");
        let script = a_stub_agent_script(dir.path(), "agent.sh")
            .recording_argv_to(&argv_file)
            .build();

        // When
        run(&script, &["--model", "sonnet", "--resume", "chat-1"]);

        // Then
        assert_eq!(
            read_recorded_argv(&argv_file).unwrap(),
            vec!["--model", "sonnet", "--resume", "chat-1"]
        );
        assert_eq!(
            read_recorded_argv_value_after(&argv_file, "--resume").unwrap(),
            "chat-1"
        );
    }

    #[test]
    fn a_recording_stub_leaves_no_temp_file_behind() {
        // Given — the temp file is an implementation detail of the atomic write; a reader
        // globbing the directory must not trip over it
        let dir = a_temp_dir();
        let argv_file = dir.path().join("argv.txt");
        let script = a_stub_agent_script(dir.path(), "agent.sh")
            .recording_argv_to(&argv_file)
            .build();

        // When
        run(&script, &["--model", "sonnet"]);

        // Then
        let leftovers: Vec<PathBuf> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().contains(".tmp."))
            .collect();
        assert_eq!(leftovers, Vec::<PathBuf>::new());
    }

    #[test]
    fn an_appending_stub_keeps_one_whole_line_per_invocation() {
        // Given
        let dir = a_temp_dir();
        let argv_log = dir.path().join("argv.log");
        let script = a_stub_agent_script(dir.path(), "agent.sh")
            .appending_argv_to(&argv_log)
            .build();

        // When — two invocations, the second the one under test
        run(&script, &["--resume", "chat-1"]);
        run(&script, &["--resume", "chat-2", "--model", "sonnet"]);

        // Then
        assert_eq!(appended_invocation_count(&argv_log), 2);
        assert_eq!(
            read_last_appended_argv(&argv_log).unwrap(),
            vec!["--resume", "chat-2", "--model", "sonnet"]
        );
    }

    #[test]
    fn an_appending_stub_writes_each_invocation_in_a_single_append() {
        // Given — the property that makes a partially-observed argv impossible. A torn record
        // would show up as more lines than invocations, which is exactly the failure this
        // module exists to prevent.
        let dir = a_temp_dir();
        let argv_log = dir.path().join("argv.log");
        let script = a_stub_agent_script(dir.path(), "agent.sh")
            .appending_argv_to(&argv_log)
            .build();

        // When — many invocations, each with several arguments
        for i in 0..20 {
            run(&script, &["--resume", &format!("chat-{i}"), "--model", "x"]);
        }

        // Then
        assert_eq!(appended_invocation_count(&argv_log), 20);
    }

    #[test]
    fn a_prelude_runs_before_the_argv_is_recorded() {
        // Given — a stub that answers a subcommand and exits without recording, the shape the
        // cursor-agent stub needs for `create-chat`
        let dir = a_temp_dir();
        let argv_log = dir.path().join("argv.log");
        let script = a_stub_agent_script(dir.path(), "agent.sh")
            .with_prelude("if [ \"$1\" = \"create-chat\" ]; then echo minted; exit 0; fi")
            .appending_argv_to(&argv_log)
            .build();

        // When
        run(&script, &["create-chat"]);

        // Then
        assert_eq!(appended_invocation_count(&argv_log), 0);
    }

    #[test]
    fn reading_an_argv_file_that_was_never_written_reports_the_path() {
        // Given
        let dir = a_temp_dir();
        let argv_file = dir.path().join("never-written.txt");

        // When
        let observed = read_recorded_argv(&argv_file).expect_err("no stub ever ran");

        // Then — the diagnosis names the file, so a timeout says which stub stayed silent
        assert!(
            observed.contains("never-written.txt"),
            "expected the path in the diagnosis, got: {observed}"
        );
    }
}

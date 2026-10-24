//! Running a program as another OS user, and finding it on the child's `PATH`.
//!
//! These nine functions are the **only** part of the daemon's spawn layer that a subsystem crate
//! needs. `#unbundle` node 1 lifted exactly them rather than `spawner.rs`, because the file is
//! 2,539 lines of session-spawn machinery — supervisor brokering, LiveKit credentials, log config,
//! startup watches — that families E, F, G and H never touch, and because `## Boundaries` reserves
//! the spawn subsystem for node 3. What is here is what `host_tooling`, `ssh_agent`,
//! `host_private_key` and `remote_git_service` actually call: three entry points
//! ([`find_program_on_spawn_child_path`], [`run_capture_as_user`], [`start_output_as_user`]) and
//! the transitive closure that makes them work.
//!
//! `spawner.rs` re-exports every name, so no caller in `tddy-daemon` changed and there is exactly
//! one definition of "drop privileges and exec".
//!
//! Everything that actually drops privileges is `#[cfg(unix)]`: `setgroups`/`setgid`/`setuid` have
//! no meaning elsewhere, and a non-unix build must fail to find the function rather than silently
//! get one that did not drop anything.

use std::path::{Path, PathBuf};

#[cfg(unix)]
use crate::pre_exec_stages::StagedCommand;
#[cfg(unix)]
use crate::privilege_drop::{plan_privilege_change, resolve_target_account};
use std::process::Stdio;

/// The variables `gh` honours ahead of its own login.
const GITHUB_TOKEN_VARIABLES: [&str; 4] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
];

/// Resolve `tool_path` (from `dev.daemon.yaml`'s `allowed_tools`, e.g. `"target/debug/tddy-coder"`)
/// to an absolute path anchored to the **daemon's own toolchain root** — never to the target
/// session's `repo_path`.
///
/// The daemon and the `tddy-coder` it spawns are built from the same checkout/workspace (the
/// same `cargo build`, the same `target/` directory) — which *project* a session happens to
/// operate on must not change which binary gets executed. A `repo_path` is just an arbitrary
/// target codebase (it could be a TODO app); it is not expected to contain a `tddy-coder` build
/// of its own, and resolving `tool_path` against it was the bug (a session against
/// `main_repo_path`s that don't coincidentally contain a `tddy-coder` build would spawn the
/// wrong binary — or nothing at all).
///
/// An already-absolute `tool_path` is returned unchanged (an operator override).
pub fn resolve_tool_path(tool_path: &str, daemon_toolchain_root: &Path) -> PathBuf {
    resolve_relative_to_daemon_toolchain_root(Path::new(tool_path), daemon_toolchain_root)
}

/// Shared absolute/relative resolution rule for [`resolve_tool_path`] and [`resolve_tddy_data_dir`]:
/// an absolute `path` is returned unchanged (an operator override); a relative `path` is joined
/// onto `daemon_toolchain_root` (the daemon's own process cwd — never the target session's
/// `repo_path`).
pub fn resolve_relative_to_daemon_toolchain_root(
    path: &Path,
    daemon_toolchain_root: &Path,
) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        daemon_toolchain_root.join(path)
    }
}

/// Merge the daemon process `PATH` with an optional prefix (from the target user's `~/.tddy/config.yaml`).
pub fn merge_spawn_child_path(path_extra: Option<&str>) -> String {
    const FALLBACK: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
    let base = std::env::var("PATH").unwrap_or_else(|_| FALLBACK.to_string());
    let Some(extra) = path_extra.map(str::trim).filter(|s| !s.is_empty()) else {
        return base;
    };
    format!("{}:{}", extra.trim_end_matches(':'), base)
}

/// The absolute path of `program` on the same `PATH` a spawned child is given, or `None`.
///
/// [`run_output_as_user`] resolves a *relative* program against the daemon's own toolchain root and
/// never consults `PATH` — deliberately, so an operator's configured tool path cannot be shadowed
/// by whatever happens to be on the target user's `PATH` (see [`resolve_tool_path`]). A caller that
/// wants a tool *by name* (`git`, `gh`) therefore has to do the lookup itself, and it has to use
/// the `PATH` the child will actually run with, which is [`merge_spawn_child_path`]'s.
///
/// `None` is a finding, not an error: no entry on that `PATH` holds an executable file of that
/// name. What that means — "not installed" for one tool, "this probe cannot run" for another — is
/// the caller's to decide, because only the caller knows whether the tool's absence is itself an
/// answer.
#[cfg(unix)]
pub fn find_program_on_spawn_child_path(program: &str) -> Option<PathBuf> {
    find_program_on_path(program, &merge_spawn_child_path(None))
}

/// [`find_program_on_spawn_child_path`] with the `PATH` to search passed in rather than read from
/// the environment.
///
/// The split is what makes the lookup testable. Reading `std::env` here would leave a test no way
/// to decide what is on `PATH` except to set the process-wide variable — and `PATH` is process
/// state shared with every other test running in parallel, so a test that swapped it would break
/// whichever unrelated test happened to resolve a binary at that moment. `#[serial]` does not save
/// it either: that serializes a test only against other `#[serial]` tests, not against the several
/// hundred running concurrently beside them. The result is an intermittent failure somewhere else
/// entirely, which is the worst kind to own.
///
/// Same reasoning as `tddy_host_service::host_tooling::classify_gh_auth_status`: the part that is worth
/// pinning is a pure function of its inputs, so it takes them as arguments.
#[cfg(unix)]
fn find_program_on_path(program: &str, path_value: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    path_value
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(program))
        .find(|candidate| {
            // `metadata` follows symlinks, which is what `execve` does too — a `PATH` entry is
            // usually a symlink into a package's own tree.
            std::fs::metadata(candidate)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
}

/// What [`run_output_as_user`] made of an attempt to run a program as another user.
///
/// The three outcomes stay separate because callers act on them differently: a program that never
/// started is not installed, one that exited non-zero ran and disagreed, and a setup failure says
/// nothing about the program at all.
#[cfg(unix)]
pub struct CaptureAsUser {
    /// The path actually executed, after `resolve_tool_path` — what an error message should name.
    pub resolved_program: PathBuf,
    /// `Err` when the program could not be started (not on `PATH`, not executable, or the
    /// privilege drop failed), which is *not* the same as a program that ran and exited non-zero.
    pub result: std::io::Result<std::process::Output>,
}

/// A command built to run as another OS user, and not yet started.
///
/// Kept together because the two travel together: an error message names the program that was
/// actually resolved, which is not the one the caller passed whenever `resolve_tool_path` anchored
/// a relative path.
#[cfg(unix)]
struct AsUserCommand {
    resolved_program: PathBuf,
    staged: StagedCommand,
}

/// Build `program args...` to run as `os_user` — program resolution, the passwd lookup, `HOME`,
/// `PATH`, the working directory and the privilege drop.
///
/// Shared by [`run_output_as_user`] and [`start_output_as_user`] rather than written twice: none of
/// it is optional, and a second copy that drifted would produce a child running as the wrong user
/// or reading the wrong `$HOME` — which is precisely the answer these callers exist to get right.
#[cfg(unix)]
fn as_user_command(
    os_user: &str,
    program: &Path,
    args: &[String],
) -> anyhow::Result<AsUserCommand> {
    // Anchor a relative `program` to the daemon's own toolchain root (its own process cwd —
    // nothing in this crate ever calls set_current_dir, so this reliably reflects wherever
    // ./web-dev / cargo run -p tddy-daemon was launched from), not to the target OS user's home
    // directory this function `.current_dir()`s into below before exec-ing. Same rationale as
    // `spawn_as_user`'s `resolve_tool_path` call.
    let daemon_toolchain_root = std::env::current_dir()
        .map_err(|e| anyhow::anyhow!("resolve daemon's own toolchain root (current_dir): {}", e))?;
    let resolved_program = resolve_tool_path(&program.to_string_lossy(), &daemon_toolchain_root);

    let target = resolve_target_account(os_user).map_err(anyhow::Error::msg)?;
    let home_dir = target.home.clone();
    let change = plan_privilege_change(target)?;

    let mut cmd = std::process::Command::new(&resolved_program);
    cmd.args(args)
        // Run from the target user's home: the daemon's own cwd may be unreadable after setuid, and
        // the ACP probe path opens a session against the current directory.
        .current_dir(&home_dir)
        .env("HOME", &home_dir)
        .env("PATH", merge_spawn_child_path(None))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // The environment is never a credential: a daemon that has one of these exported must not hand
    // it to a child run as a user, or `gh` would act as whoever exported it.
    for token_variable in GITHUB_TOKEN_VARIABLES {
        cmd.env_remove(token_variable);
    }

    let label = format!("{} as {os_user}", resolved_program.display());
    Ok(AsUserCommand {
        resolved_program,
        staged: StagedCommand::new(cmd, &label, change),
    })
}

/// Run `program args...` as the target OS user and hand back its whole result — exit status,
/// stdout and stderr.
///
/// [`run_capture_as_user`] is this with the non-success cases collapsed into an error. A caller
/// that has to *classify* an exit code, or read a tool that reports on stderr (`gh auth status`
/// does), needs the parts kept apart, and the user resolution / `HOME` / privilege drop is the same
/// either way.
///
/// The wait here is unconditional: `Command::output` returns when the child does, and a caller that
/// cannot afford to wait that long wants [`start_output_as_user`] instead.
#[cfg(unix)]
pub fn run_output_as_user(
    os_user: &str,
    program: &Path,
    args: &[String],
) -> anyhow::Result<CaptureAsUser> {
    let AsUserCommand {
        resolved_program,
        staged,
    } = as_user_command(os_user, program, args)?;
    let result = staged
        .spawn()
        .and_then(|started| started.child.wait_with_output());
    Ok(CaptureAsUser {
        resolved_program,
        result,
    })
}

/// A child started by [`start_output_as_user`] and still running, with its pipes already detached.
#[cfg(unix)]
pub struct SpawnedAsUser {
    /// The path actually executed, after `resolve_tool_path` — what an error message should name.
    pub resolved_program: PathBuf,
    /// The live child, for the caller to own. Owning it is the whole point: it is the only handle
    /// that can end a command which overruns a deadline, and the only one that can `wait()` it
    /// afterwards so the kernel does not keep it as a zombie.
    pub child: std::process::Child,
    /// Detached here instead of being left on `child`, so a caller can hand the pipes to a reader
    /// thread while keeping the kill handle for itself. The two are not separable afterwards.
    pub stdout: std::process::ChildStdout,
    /// The child's stderr — kept apart from stdout because callers classify the two differently.
    pub stderr: std::process::ChildStderr,
}

/// Start `program args...` as the target OS user and hand back the **running** child.
///
/// [`run_output_as_user`] is this plus a wait that nothing can interrupt: it hands back no handle,
/// so a caller that gives up on a slow child cannot end it. The process, the thread waiting on it
/// and its two pipe descriptors then outlive the caller's deadline — which for a daemon that probes
/// every host it lists, on every poll, is a descriptor leak rather than a slow answer.
#[cfg(unix)]
pub fn start_output_as_user(
    os_user: &str,
    program: &Path,
    args: &[String],
) -> anyhow::Result<SpawnedAsUser> {
    let AsUserCommand {
        resolved_program,
        staged,
    } = as_user_command(os_user, program, args)?;
    // The stage watcher ends by itself once the child execs, so the child alone is handed back.
    let mut child = staged.spawn()?.child;
    match (child.stdout.take(), child.stderr.take()) {
        (Some(stdout), Some(stderr)) => Ok(SpawnedAsUser {
            resolved_program,
            child,
            stdout,
            stderr,
        }),
        // `as_user_command` pipes both streams and nothing else has taken them, so this cannot
        // happen — but a child we refuse to hand back is still a child we started, and returning
        // an error while dropping it would leak exactly what this function exists to prevent.
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "{} started without the pipes it was configured with",
                resolved_program.display()
            )
        }
    }
}

/// Run `program args...` as the target OS user and capture stdout. Errors (including non-zero
/// exit) carry stderr so a failing probe surfaces the underlying cause rather than an empty result.
#[cfg(unix)]
pub fn run_capture_as_user(
    os_user: &str,
    program: &Path,
    args: &[String],
) -> anyhow::Result<String> {
    let run = run_output_as_user(os_user, program, args)?;
    let output = run
        .result
        .map_err(|e| anyhow::anyhow!("{}: {}", run.resolved_program.display(), e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "{} exited with {}: {}",
            run.resolved_program.display(),
            output.status,
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
/// The `PATH` lookup callers need *because* `resolve_tool_path` deliberately has none.
#[cfg(all(test, unix))]
mod find_program_on_path_tests {
    use super::find_program_on_path;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    fn an_executable_named(name: &str, in_dir: &Path) -> PathBuf {
        let path = in_dir.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn a_path_of(dirs: &[&Path]) -> String {
        dirs.iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(":")
    }

    /// The whole point: a bare program name becomes an absolute path, which is the only form
    /// `run_output_as_user` resolves without joining it onto the daemon's own cwd.
    #[test]
    fn finds_an_executable_on_the_path_and_returns_it_absolute() {
        // Given — a PATH whose second entry holds the program
        let dir = tempfile::tempdir().unwrap();
        let expected = an_executable_named("tddy-probe-subject", dir.path());
        let path = format!("/nonexistent-first:{}", dir.path().display());

        // When
        let found = find_program_on_path("tddy-probe-subject", &path);

        // Then
        assert_eq!(
            found,
            Some(expected),
            "a name on PATH resolves to the absolute path of the entry that holds it"
        );
    }

    /// A name nothing on `PATH` holds reports absence rather than inventing a path that would
    /// then fail at exec time as an unrelated-looking error.
    #[test]
    fn reports_absence_for_a_program_no_path_entry_holds() {
        // Given — a PATH of one empty directory
        let dir = tempfile::tempdir().unwrap();

        // When / Then
        assert_eq!(
            find_program_on_path("tddy-probe-subject", &a_path_of(&[dir.path()])),
            None,
            "an empty PATH entry holds nothing, and that is the answer"
        );
    }

    /// A non-executable file of the right name is not the program: `execve` would refuse it, so
    /// reporting it found would turn "not installed" into a permission error one layer down.
    #[test]
    fn skips_a_matching_name_that_is_not_executable() {
        // Given — a file of the right name that cannot be executed
        let dir = tempfile::tempdir().unwrap();
        let not_executable = dir.path().join("tddy-probe-subject");
        std::fs::write(&not_executable, "not a program\n").unwrap();
        std::fs::set_permissions(&not_executable, std::fs::Permissions::from_mode(0o644)).unwrap();

        // When / Then
        assert_eq!(
            find_program_on_path("tddy-probe-subject", &a_path_of(&[dir.path()])),
            None,
            "a file that cannot be executed is not a program that is installed"
        );
    }

    /// Earlier `PATH` entries win, the way the shell and `execvp` resolve — otherwise the probe
    /// would run a different binary than everything else on the host does.
    #[test]
    fn prefers_the_earliest_path_entry_that_holds_the_program() {
        // Given — two PATH entries, both holding an executable of the same name
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let expected = an_executable_named("tddy-probe-subject", first.path());
        an_executable_named("tddy-probe-subject", second.path());

        // When / Then
        assert_eq!(
            find_program_on_path(
                "tddy-probe-subject",
                &a_path_of(&[first.path(), second.path()])
            ),
            Some(expected)
        );
    }

    /// An empty `PATH` is not a crash and not a match — `merge_spawn_child_path` falls back to a
    /// real list, but the lookup must not assume it was handed one.
    #[test]
    fn reports_absence_for_an_empty_path() {
        assert_eq!(find_program_on_path("tddy-probe-subject", ""), None);
    }
}

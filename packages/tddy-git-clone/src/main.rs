//! `tddy-git-clone --git <git> -- <url> <destination>`: one clone, as whichever OS user runs it.
//!
//! On a supervised host the daemon asks `tddy-supervisor` to run this helper as the session user
//! (`SpawnStreamed`, `#agent-tools` 32/32), so everything a clone does to the filesystem is done by
//! that user and the supervisor knows nothing about git:
//!
//! - `git clone --progress -- <url> <destination>.partial-<random>`, beside the destination so the
//!   rename stays on one filesystem; git's stderr — its progress and its own explanation of a
//!   failure — passes through on this process's stderr, which the supervisor streams line by line;
//! - on success, the partial directory is renamed to `<destination>`;
//! - on failure (or a stop), the partial directory is removed, so a destination either does not
//!   exist or holds a complete clone;
//! - a destination that already exists is refused, and left alone.
//!
//! It exits with git's exit code, or non-zero when it refused or could not rename.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many partial-directory names are tried before the helper gives up.
const PARTIAL_NAME_ATTEMPTS: u32 = 100;

/// Why the helper did not leave a complete clone at the destination, and the exit it reports.
struct Refusal {
    message: Option<String>,
    exit: ExitCode,
}

impl Refusal {
    /// A refusal the helper explains itself on stderr.
    fn said(message: String) -> Refusal {
        Refusal {
            message: Some(message),
            exit: ExitCode::from(1),
        }
    }

    /// Git failed and already said why on stderr; the helper passes its exit on.
    fn git_exited(code: Option<i32>) -> Refusal {
        let exit = match code.and_then(|code| u8::try_from(code).ok()) {
            Some(code) if code != 0 => ExitCode::from(code),
            _ => ExitCode::from(1),
        };
        Refusal {
            message: None,
            exit,
        }
    }
}

fn random_suffix(attempt: u32) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", std::process::id(), nanos, attempt)
}

/// A name beside `destination` that nothing holds yet. Every attempt taken is a refusal, never a
/// name that might already be someone else's.
fn partial_sibling(destination: &Path) -> Result<PathBuf, Refusal> {
    (0..PARTIAL_NAME_ATTEMPTS)
        .map(|attempt| {
            let mut name: OsString = destination.as_os_str().to_owned();
            name.push(format!(".partial-{}", random_suffix(attempt)));
            PathBuf::from(name)
        })
        .find(|partial| std::fs::symlink_metadata(partial).is_err())
        .ok_or_else(|| {
            Refusal::said(format!(
                "no free partial directory name beside {} after {PARTIAL_NAME_ATTEMPTS} attempts",
                destination.display()
            ))
        })
}

fn remove_partial(partial: &Path) {
    match std::fs::symlink_metadata(partial) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            let _ = std::fs::remove_dir_all(partial);
        }
        Ok(_) => {
            let _ = std::fs::remove_file(partial);
        }
        Err(_) => {}
    }
}

/// The helper's one invocation: `--git <git> -- <url> <destination>`.
struct Invocation {
    git: PathBuf,
    url: OsString,
    destination: PathBuf,
}

fn parse_args(args: Vec<OsString>) -> Result<Invocation, String> {
    match <[OsString; 5]>::try_from(args) {
        Ok([flag, git, separator, url, destination]) if flag == "--git" && separator == "--" => {
            Ok(Invocation {
                git: PathBuf::from(git),
                url,
                destination: PathBuf::from(destination),
            })
        }
        _ => Err("usage: tddy-git-clone --git <git> -- <url> <destination>".to_string()),
    }
}

fn refuse_an_existing(destination: &Path) -> Result<(), Refusal> {
    match std::fs::symlink_metadata(destination) {
        Ok(_) => Err(Refusal::said(format!(
            "destination already exists: {}",
            destination.display()
        ))),
        Err(_) => Ok(()),
    }
}

/// Run git's clone into `partial`; its progress and its own explanation of a failure pass through.
fn git_clone(git: &Path, url: &OsString, partial: &Path) -> Result<(), Refusal> {
    let status = Command::new(git)
        .arg("clone")
        .arg("--progress")
        // Whatever follows is a URL, even one that starts with a dash.
        .arg("--")
        .arg(url)
        .arg(partial)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| Refusal::said(format!("failed to run {}: {error}", git.display())))?;
    if status.success() {
        Ok(())
    } else {
        Err(Refusal::git_exited(status.code()))
    }
}

/// Clone into a fresh partial sibling and move it into place; the partial directory is removed on
/// every path that does not end in the rename.
fn clone_into_place(invocation: &Invocation) -> Result<(), Refusal> {
    let destination = &invocation.destination;
    refuse_an_existing(destination)?;
    let partial = partial_sibling(destination)?;
    let placed = git_clone(&invocation.git, &invocation.url, &partial)
        .and_then(|()| refuse_an_existing(destination))
        .and_then(|()| {
            std::fs::rename(&partial, destination).map_err(|error| {
                Refusal::said(format!(
                    "failed to rename {} to {}: {error}",
                    partial.display(),
                    destination.display()
                ))
            })
        });
    if placed.is_err() {
        remove_partial(&partial);
    }
    placed
}

fn main() -> ExitCode {
    let invocation = match parse_args(std::env::args_os().skip(1).collect()) {
        Ok(invocation) => invocation,
        Err(usage) => {
            eprintln!("tddy-git-clone: {usage}");
            return ExitCode::from(2);
        }
    };
    match clone_into_place(&invocation) {
        Ok(()) => ExitCode::SUCCESS,
        Err(refusal) => {
            if let Some(message) = refusal.message {
                eprintln!("tddy-git-clone: {message}");
            }
            refusal.exit
        }
    }
}

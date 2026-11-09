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

fn random_suffix(attempt: u32) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", std::process::id(), nanos, attempt)
}

fn partial_sibling(destination: &Path) -> PathBuf {
    for attempt in 0..100 {
        let mut name: OsString = destination.as_os_str().to_owned();
        name.push(format!(".partial-{}", random_suffix(attempt)));
        let partial = PathBuf::from(name);
        if std::fs::symlink_metadata(&partial).is_err() {
            return partial;
        }
    }
    // Extremely unlikely fallback: still return a partial-shaped path and let
    // git report the collision; the failure path removes nothing that is not
    // this clone's target more than git itself would have used.
    let mut name: OsString = destination.as_os_str().to_owned();
    name.push(format!(".partial-{}", random_suffix(100)));
    PathBuf::from(name)
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

fn parse_args() -> Result<(PathBuf, String, PathBuf), String> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    // Expected: --git <git> -- <url> <destination>
    if args.len() != 5 || args[0] != "--git" || args[2] != "--" {
        return Err("usage: tddy-git-clone --git <git> -- <url> <destination>".to_string());
    }
    let git = PathBuf::from(&args[1]);
    let url = args[3].to_string_lossy().into_owned();
    let destination = PathBuf::from(&args[4]);
    Ok((git, url, destination))
}

fn run() -> ExitCode {
    let (git, url, destination) = match parse_args() {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("tddy-git-clone: {message}");
            return ExitCode::from(2);
        }
    };

    if std::fs::symlink_metadata(&destination).is_ok() {
        eprintln!(
            "tddy-git-clone: destination already exists: {}",
            destination.display()
        );
        return ExitCode::from(1);
    }

    let partial = partial_sibling(&destination);

    let status = Command::new(&git)
        .arg("clone")
        .arg("--progress")
        .arg("--")
        .arg(&url)
        .arg(&partial)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    let status = match status {
        Ok(status) => status,
        Err(error) => {
            eprintln!("tddy-git-clone: failed to run {}: {error}", git.display());
            remove_partial(&partial);
            return ExitCode::from(1);
        }
    };

    if !status.success() {
        remove_partial(&partial);
        return match status.code() {
            Some(code) if code != 0 => ExitCode::from(u8::try_from(code).unwrap_or(1)),
            _ => ExitCode::from(1),
        };
    }

    if std::fs::symlink_metadata(&destination).is_ok() {
        eprintln!(
            "tddy-git-clone: destination already exists: {}",
            destination.display()
        );
        remove_partial(&partial);
        return ExitCode::from(1);
    }

    match std::fs::rename(&partial, &destination) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "tddy-git-clone: failed to rename {} to {}: {error}",
                partial.display(),
                destination.display()
            );
            remove_partial(&partial);
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    run()
}

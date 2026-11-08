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

use std::process::ExitCode;

fn main() -> ExitCode {
    // TODO(supervisor-agent-agnostic): implement — clone into a partial sibling, rename into place,
    // remove the partial on failure, refuse an existing destination. Until then it touches nothing.
    eprintln!("tddy-git-clone: not implemented yet");
    ExitCode::from(2)
}

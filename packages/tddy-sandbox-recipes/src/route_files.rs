//! A routed jail's route files, written into the jail's scratch directory owner-only.
//!
//! A route file can carry the endpoint and the key a harness runs on (a harness's configuration
//! may name its provider there), so it is written owner-only: a `0700` directory, `0600` files created exclusively, and a name that is one plain file name. The
//! directory is the scratch's own, so it is replaced when a plan is built again for the same
//! session (a relaunch).

use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use tddy_core::provider_route::HarnessRoute;
use tddy_sandbox::SandboxError;

/// The directory under the jail's scratch that holds a routed session's files.
const ROUTE_FILES_DIR: &str = "provider-route";
const OWNER_ONLY_DIR: u32 = 0o700;
const OWNER_ONLY_FILE: u32 = 0o600;

/// Write `route`'s files into `scratch_dir/provider-route` and return the path of the first one,
/// which the launch's configuration variable names (`coding_agent.route.config_env`). `None` when the route has no files, and
/// nothing is written.
pub(crate) fn materialise_route_files(
    scratch_dir: &Path,
    route: &HarnessRoute,
) -> Result<Option<PathBuf>, SandboxError> {
    let Some(first) = route.files.first() else {
        return Ok(None);
    };
    // Every name is checked before anything is written, so a refused name leaves nothing behind.
    for file in &route.files {
        plain_file_name(&file.name)?;
    }
    let dir = scratch_dir.join(ROUTE_FILES_DIR);
    std::fs::create_dir_all(scratch_dir)
        .map_err(|e| io_error("create the scratch", scratch_dir, &e))?;
    remove_stale_dir(&dir)?;
    std::fs::DirBuilder::new()
        .mode(OWNER_ONLY_DIR)
        .create(&dir)
        .map_err(|e| io_error("create the route files directory", &dir, &e))?;
    for file in &route.files {
        let path = dir.join(plain_file_name(&file.name)?);
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(OWNER_ONLY_FILE)
            .open(&path)
            .map_err(|e| io_error("create a route file", &path, &e))?;
        out.write_all(file.contents.as_bytes())
            .map_err(|e| io_error("write a route file", &path, &e))?;
    }
    Ok(Some(dir.join(plain_file_name(&first.name)?)))
}

/// `name` when it is exactly one normal path component. A separator or a parent reference would
/// write outside the route directory, and an empty or dot name names no file.
fn plain_file_name(name: &str) -> Result<&str, SandboxError> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(_)), None) => Ok(name),
        _ => Err(SandboxError::InvalidSpec(format!(
            "a provider route file must be named by a plain file name, not {name:?}"
        ))),
    }
}

/// Remove a directory a previous plan for this scratch wrote. A symbolic link is removed, not
/// followed: `remove_dir_all` does not descend through one.
fn remove_stale_dir(dir: &Path) -> Result<(), SandboxError> {
    match std::fs::symlink_metadata(dir) {
        Ok(_) => std::fs::remove_dir_all(dir)
            .map_err(|e| io_error("replace the route files directory", dir, &e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error("inspect the route files directory", dir, &e)),
    }
}

fn io_error(action: &str, path: &Path, error: &std::io::Error) -> SandboxError {
    SandboxError::Io(format!("{action} {}: {error}", path.display()))
}

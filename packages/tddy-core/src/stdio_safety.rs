//! Guarantees a process's stdin/stdout stay dedicated to RPC framing when running in `--stdio`
//! mode (see `tddy-stdio`), which has zero tolerance for stray bytes on the peer's stdout.

use crate::log_backend::{LogConfig, LogOutput};

/// Force any `LogOutput::Stdout` logger destination in `config` to `LogOutput::Stderr`, since
/// `--stdio` dedicates fd 1 to RPC framing. Leaves every other output (`Stderr`, `File`, `Buffer`,
/// `Mute`) untouched. Returns the number of loggers changed.
pub fn enforce_stdio_safe_log_output(config: &mut LogConfig) -> usize {
    let mut overridden = 0;
    for logger in config.loggers.values_mut() {
        if matches!(logger.output, LogOutput::Stdout) {
            logger.output = LogOutput::Stderr;
            overridden += 1;
        }
    }
    overridden
}

/// Redirect `target_fd` to `path`: creates (or truncates) the file, then makes `target_fd` an
/// alias of it via `dup2`, replacing whatever `target_fd` pointed to before. Used to redirect
/// stderr to a log file in `--stdio` mode (stdin/stdout must stay live, unlike `--daemon`'s
/// stdin/stdout/stderr-null headless mode).
///
/// The file is created before any `dup2` call, so `target_fd` is left untouched if the file
/// can't be created.
#[cfg(unix)]
pub fn redirect_fd_to_file(
    target_fd: std::os::unix::io::RawFd,
    path: &std::path::Path,
) -> std::io::Result<()> {
    use std::os::unix::io::IntoRawFd;

    let file = std::fs::File::create(path)?;
    let fd = file.into_raw_fd();
    let ret = unsafe { libc::dup2(fd, target_fd) };
    unsafe {
        libc::close(fd);
    }
    if ret == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

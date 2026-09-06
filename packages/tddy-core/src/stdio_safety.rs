//! Guarantees a process's stdin/stdout stay dedicated to RPC framing when running in `--stdio`
//! mode (see `tddy-stdio`), which has zero tolerance for stray bytes on the peer's stdout.

use crate::log_backend::{LogConfig, LogOutput};

/// Force any `LogOutput::Stdout` logger destination in `config` to `LogOutput::Stderr`, since
/// `--stdio` dedicates fd 1 to RPC framing. That includes a `Stdout` nested in a `LogOutput::Many`
/// fan-out, which would corrupt the protocol just as surely as a lone one. Leaves every other
/// output (`Stderr`, `File`, `Buffer`, `Mute`) untouched. Returns the number of loggers changed.
pub fn enforce_stdio_safe_log_output(config: &mut LogConfig) -> usize {
    let mut overridden = 0;
    for logger in config.loggers.values_mut() {
        if let Some(safe) = stdio_safe_output(&logger.output) {
            logger.output = safe;
            overridden += 1;
        }
    }
    overridden
}

/// The stdio-safe form of `output`, or `None` when it is already safe. A fan-out is rebuilt
/// through [`LogOutput::fan_out`] so that overriding, say, `[stdout, stderr]` leaves one stderr
/// destination rather than two writing every line twice.
fn stdio_safe_output(output: &LogOutput) -> Option<LogOutput> {
    match output {
        LogOutput::Stdout => Some(LogOutput::Stderr),
        LogOutput::Many(destinations) => {
            if !destinations.iter().any(|d| stdio_safe_output(d).is_some()) {
                return None;
            }
            let safe: Vec<LogOutput> = destinations
                .iter()
                .map(|d| stdio_safe_output(d).unwrap_or_else(|| d.clone()))
                .collect();
            LogOutput::fan_out(safe)
        }
        LogOutput::Stderr | LogOutput::File(_) | LogOutput::Buffer | LogOutput::Mute => None,
    }
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

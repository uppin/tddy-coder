//! What the command line asked the application binary for.
//!
//! The application takes no arguments, so anything on its command line is a mistake — and running
//! it is never free: it opens a window and hosts a whole daemon over the data dir. A
//! `tddy-desktop --help` once started a second instance beside the running one, which took over
//! that instance's agent-tool socket. `--help` and `--version` answer and exit; anything else is
//! refused before anything starts.

use std::ffi::OsString;

/// What one start of the binary is for.
#[derive(Debug, PartialEq, Eq)]
pub enum Invocation {
    /// Open the application.
    Run,
    /// Print [`help`] and exit.
    Help,
    /// Print [`version`] and exit.
    Version,
}

/// Read the command line, without the program name.
pub fn from_args(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    // macOS's launcher may hand an application its process serial number; it asks for nothing.
    let mut args = args
        .into_iter()
        .filter(|arg| !arg.to_string_lossy().starts_with("-psn_"));
    let Some(first) = args.next() else {
        return Ok(Invocation::Run);
    };
    let invocation = match first.to_str() {
        Some("--help" | "-h") => Invocation::Help,
        Some("--version" | "-V") => Invocation::Version,
        _ => return Err(unrecognised(&first)),
    };
    match args.next() {
        None => Ok(invocation),
        Some(extra) => Err(unrecognised(&extra)),
    }
}

fn unrecognised(arg: &OsString) -> String {
    format!(
        "tddy-desktop: unrecognised argument {:?}; it takes none (see --help)",
        arg.to_string_lossy()
    )
}

/// The `--help` text.
pub fn help() -> String {
    format!(
        "Tddy Desktop {}\n\n\
         Opens the Tddy Desktop window, which hosts a tddy-daemon in this process. It takes no \
         arguments: its configuration is ~/.tddy/desktop.yaml.\n\n\
         Usage: tddy-desktop [--help | --version]\n",
        env!("CARGO_PKG_VERSION")
    )
}

/// The `--version` text.
pub fn version() -> String {
    format!("tddy-desktop {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoked_with(args: &[&str]) -> Result<Invocation, String> {
        from_args(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_open_the_application() {
        assert_eq!(invoked_with(&[]), Ok(Invocation::Run));
    }

    #[test]
    fn asking_for_help_or_the_version_opens_nothing() {
        assert_eq!(
            [
                invoked_with(&["--help"]),
                invoked_with(&["-h"]),
                invoked_with(&["--version"]),
                invoked_with(&["-V"]),
            ],
            [
                Ok(Invocation::Help),
                Ok(Invocation::Help),
                Ok(Invocation::Version),
                Ok(Invocation::Version),
            ]
        );
    }

    #[test]
    fn an_argument_it_does_not_take_is_refused_rather_than_ignored() {
        assert!([
            invoked_with(&["--config"]),
            invoked_with(&["serve"]),
            invoked_with(&["--help", "extra"]),
        ]
        .iter()
        .all(Result::is_err));
    }

    #[test]
    fn the_process_serial_number_macos_hands_an_application_is_not_an_argument() {
        assert_eq!(invoked_with(&["-psn_0_12345"]), Ok(Invocation::Run));
    }
}

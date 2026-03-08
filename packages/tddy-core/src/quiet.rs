//! Quiet mode: when TDDY_QUIET is set, debug output is suppressed.
//! Used during TUI to avoid corrupting the terminal display.

#[macro_export]
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        if std::env::var("TDDY_QUIET").is_err() {
            eprintln!($($arg)*);
        }
    };
}

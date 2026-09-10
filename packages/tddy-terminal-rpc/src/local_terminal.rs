//! The local terminal a relay takes over: its size, and raw mode for the duration.
//!
//! Both relays in this crate need exactly this and nothing more — [`crate::local_pty_relay`] for a
//! PTY in this process, [`crate::pty_relay`] for a session's terminal on the far side of gRPC or
//! LiveKit. They carried a byte-identical copy each until `#unbundle` node 5 brought the second one
//! into this crate; two `RawMode`s in one crate would have been absurd, so there is one.

/// Terminal size when `TIOCGWINSZ` cannot be read (no tty, or a zero-sized one).
const DEFAULT_ROWS: u16 = 24;
const DEFAULT_COLS: u16 = 220;

/// Read the local terminal size via `TIOCGWINSZ`, falling back to a default.
pub(crate) fn terminal_size() -> (u16, u16) {
    #[cfg(unix)]
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0
            && ws.ws_row > 0
            && ws.ws_col > 0
        {
            return (ws.ws_row, ws.ws_col);
        }
    }
    (DEFAULT_ROWS, DEFAULT_COLS)
}

/// Raw-mode guard: puts the local stdin into raw mode for the lifetime of the returned value so the
/// far side receives keystrokes verbatim. Restores the saved termios on drop.
pub(crate) struct RawMode {
    #[cfg(unix)]
    saved: libc::termios,
}

impl RawMode {
    pub(crate) fn enable() -> Self {
        #[cfg(unix)]
        unsafe {
            let mut saved: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(libc::STDIN_FILENO, &mut saved) == 0 {
                let mut raw = saved;
                libc::cfmakeraw(&mut raw);
                libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
                return Self { saved };
            }
        }
        Self {
            #[cfg(unix)]
            saved: unsafe { std::mem::zeroed() },
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved);
        }
    }
}

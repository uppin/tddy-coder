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
    // SAFETY: `libc::winsize` is a plain POD struct of four `u16`s, so all-zero is a valid value
    // for it and `zeroed()` needs no further initialisation. `TIOCGWINSZ` is the ioctl whose
    // third argument is exactly a `*mut winsize`, and `&mut ws` is a valid, uniquely borrowed,
    // correctly aligned pointer to one that outlives the call. `STDOUT_FILENO` is a borrowed fd
    // this function neither closes nor takes ownership of; an invalid or non-tty fd is reported
    // as a non-zero return, which is the branch that falls through to the defaults. `ws` is read
    // only after the call reported success.
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
        // SAFETY: `libc::termios` is a plain POD struct of integers and a byte array, so all-zero
        // is a valid value for it and `zeroed()` needs no further initialisation. `tcgetattr` and
        // `tcsetattr` take `*mut termios` / `*const termios` respectively, and `&mut saved` /
        // `&raw` are valid, correctly aligned pointers to live locals that outlive their call;
        // `cfmakeraw` takes the same `*mut termios`. `STDIN_FILENO` is a borrowed fd, neither
        // closed nor owned here. `saved` is only copied into `raw`, and `raw` only written back,
        // after `tcgetattr` reported success, so no uninitialised termios is ever installed on
        // this path. Both setters' return codes are deliberately ignored: a stdin that is not a
        // tty leaves the terminal untouched, which is the intended outcome.
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
            // SAFETY: as above, all-zero is a valid `libc::termios`. This is the arm where
            // `tcgetattr` failed, so there is no saved state to restore and nothing was changed;
            // the zeroed value exists only so the field is initialised, and `Drop`'s `tcsetattr`
            // on a non-tty stdin fails and is ignored.
            saved: unsafe { std::mem::zeroed() },
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: `tcsetattr` takes a `*const termios`, and `&self.saved` is a valid, correctly
        // aligned pointer to a field that lives until this `Drop` returns. `STDIN_FILENO` is a
        // borrowed fd, neither closed nor owned here, and the return code is ignored because a
        // stdin that is no longer a tty simply leaves the terminal as it is.
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved);
        }
    }
}

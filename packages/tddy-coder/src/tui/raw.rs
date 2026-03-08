//! Raw terminal mode with ISIG preserved so Ctrl+C generates SIGINT.
//!
//! Crossterm's enable_raw_mode() clears ISIG, so Ctrl+C is delivered as a key event
//! instead of SIGINT. When the child shares stdin, it can consume Ctrl+C before the
//! TUI, making it impossible to quit. Keeping ISIG ensures the ctrlc handler runs.

#[cfg(unix)]
mod unix {
    use std::io;
    use std::os::unix::io::AsRawFd;
    use std::sync::Mutex;

    static SAVED_TERMIOS: Mutex<Option<libc::termios>> = Mutex::new(None);

    pub fn enable_raw_mode_keep_sig() -> io::Result<()> {
        let fd = std::io::stdin().as_raw_fd();
        let mut termios: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut termios) } != 0 {
            return Err(io::Error::last_os_error());
        }
        *SAVED_TERMIOS.lock().unwrap_or_else(|e| e.into_inner()) = Some(termios);

        let mut raw = termios;
        // Disable canonical mode, echo, etc. but KEEP ISIG so Ctrl+C generates SIGINT.
        raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ECHOE | libc::ECHOK | libc::ECHONL | libc::IEXTEN);
        raw.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
        raw.c_oflag &= !(libc::OPOST);
        raw.c_cflag &= libc::CSIZE;
        raw.c_cflag |= libc::CS8;
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;

        if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn disable_raw_mode() -> io::Result<()> {
        let fd = std::io::stdin().as_raw_fd();
        if let Some(termios) = SAVED_TERMIOS.lock().unwrap_or_else(|e| e.into_inner()).take() {
            if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &termios) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

#[cfg(not(unix))]
mod unix {
    use std::io;
    pub fn enable_raw_mode_keep_sig() -> io::Result<()> {
        crossterm::terminal::enable_raw_mode()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
    pub fn disable_raw_mode() -> io::Result<()> {
        crossterm::terminal::disable_raw_mode()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
    }
}

pub use unix::{disable_raw_mode, enable_raw_mode_keep_sig};

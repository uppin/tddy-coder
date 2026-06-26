//! Demo TUI binary for terminal rendering e2e tests.
//!
//! Accepts the same CLI flags tddy-daemon passes to the claude CLI:
//!   --session-id, --permission-mode, --model
//! All are ignored; the binary just draws its PTY dimensions and responds to SIGWINCH.
//!
//! Output: "DEMO TUI W={cols} H={rows}" on a clear screen.
//! Redraws whenever it receives SIGWINCH so tests can assert the correct width after resize.

use clap::Parser;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Parser, Debug)]
#[command(name = "tddy-demo-tui")]
struct Args {
    #[arg(long)]
    session_id: Option<String>,
    #[arg(long)]
    permission_mode: Option<String>,
    #[arg(long)]
    model: Option<String>,
    // Absorb any extra positional args (initial prompt string) without erroring.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    _extra: Vec<String>,
}

#[cfg(unix)]
fn pty_size() -> (u16, u16) {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) == 0
            && ws.ws_col > 0
            && ws.ws_row > 0
        {
            (ws.ws_col, ws.ws_row)
        } else {
            (80, 24)
        }
    }
}

#[cfg(not(unix))]
fn pty_size() -> (u16, u16) {
    (80, 24)
}

fn draw(cols: u16, rows: u16) {
    // Clear screen, move cursor to top-left, then print dimensions.
    let frame = format!("\x1b[2J\x1b[HDEMO TUI W={} H={}", cols, rows);
    let _ = io::stdout().write_all(frame.as_bytes());
    let _ = io::stdout().flush();
}

static SIGWINCH_RECEIVED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_sigwinch(_: libc::c_int) {
    SIGWINCH_RECEIVED.store(true, Ordering::Relaxed);
}

#[cfg(unix)]
extern "C" fn on_shutdown(_: libc::c_int) {
    SHUTDOWN.store(true, Ordering::Relaxed);
}

fn main() {
    let _args = Args::parse();

    #[cfg(unix)]
    unsafe {
        libc::signal(
            libc::SIGWINCH,
            on_sigwinch as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGTERM,
            on_shutdown as *const () as libc::sighandler_t,
        );
        libc::signal(libc::SIGHUP, on_shutdown as *const () as libc::sighandler_t);
    }

    let (cols, rows) = pty_size();
    draw(cols, rows);

    loop {
        if SHUTDOWN.load(Ordering::Relaxed) {
            break;
        }
        if SIGWINCH_RECEIVED.swap(false, Ordering::Relaxed) {
            let (cols, rows) = pty_size();
            draw(cols, rows);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

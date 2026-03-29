//! `rsync` `--rsh` / external-subcommand dispatch.
//!
//! rsync keeps child stdin and stdout active concurrently. We decouple each direction with
//! unbounded channels so socket I/O never blocks on stdout/stdin pipe backpressure deadlocks.

use std::ffi::OsString;
use std::io;
use std::net::{Shutdown, TcpStream};
use std::path::Path;
use std::process::ExitCode;

use tddy_service::proto::remote_sandbox_v1::{OpenRsyncSessionRequest, OpenRsyncSessionResponse};

use crate::config::resolve_connect_base;
use crate::connect_client::{decode, encode, unary_proto};

fn io_err(msg: impl Into<String>) -> io::Error {
    io::Error::other(msg.into())
}

/// Environment variable: reuse an existing logical sandbox session (e.g. after `PutObject` seeding).
pub const RSYNC_SESSION_ENV: &str = "TDDY_REMOTE_RSYNC_SESSION";

const CHUNK: usize = 65536;

/// Bidirectional byte bridge between rsync's stdio pipes and the daemon TCP socket.
/// Uses `poll(2)` so neither direction can block the other (thread + channel ordering deadlocks).
#[cfg(unix)]
fn sync_byte_bridge(addr: &str) -> Result<(), io::Error> {
    use std::io::{Read, Write};
    use std::os::unix::io::AsRawFd;

    use io::ErrorKind::{BrokenPipe, NotConnected, WouldBlock};

    const STDIN_FILENO: libc::c_int = 0;
    const STDOUT_FILENO: libc::c_int = 1;

    struct NonblockGuard {
        fd: libc::c_int,
        orig_flags: libc::c_int,
    }

    impl Drop for NonblockGuard {
        fn drop(&mut self) {
            unsafe {
                libc::fcntl(self.fd, libc::F_SETFL, self.orig_flags);
            }
        }
    }

    fn enable_nonblock(fd: libc::c_int) -> io::Result<NonblockGuard> {
        let orig = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if orig < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, orig | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(NonblockGuard {
            fd,
            orig_flags: orig,
        })
    }

    let mut sock = TcpStream::connect(addr)?;
    sock.set_nonblocking(true)?;
    let sock_fd = sock.as_raw_fd();

    let _stdin_nb = enable_nonblock(STDIN_FILENO)?;
    let _stdout_nb = enable_nonblock(STDOUT_FILENO)?;

    log::info!("rsync rsh poll bridge: connected to {addr} sock_fd={sock_fd}");

    let mut pending_up: Vec<u8> = Vec::new();
    let mut pending_down: Vec<u8> = Vec::new();
    let mut stdin_eof = false;
    let mut sock_read_eof = false;
    let mut shut_wr = false;
    let mut buf = vec![0u8; CHUNK];

    loop {
        if stdin_eof && pending_up.is_empty() && !shut_wr {
            match sock.shutdown(Shutdown::Write) {
                Ok(()) => {
                    log::debug!("rsync rsh: shut down TCP write half after stdin EOF + flush");
                    shut_wr = true;
                }
                Err(e) if e.kind() == NotConnected => shut_wr = true,
                Err(e) => return Err(e),
            }
        }

        // When the remote closes the TCP connection, rsync may still hold our stdin open while
        // waiting for local cleanup. Do not block forever on stdin — exit once peer data is drained.
        if sock_read_eof && pending_up.is_empty() && pending_down.is_empty() {
            if !shut_wr {
                let _ = sock.shutdown(Shutdown::Write);
            }
            if stdin_eof {
                log::debug!("rsync rsh poll bridge: stdin EOF and TCP closed, done");
            } else {
                log::info!(
                    "rsync rsh: peer closed TCP read side; exiting bridge (stdin may still be open)"
                );
            }
            break;
        }

        let mut pfds: Vec<libc::pollfd> = Vec::with_capacity(4);

        if !stdin_eof {
            pfds.push(libc::pollfd {
                fd: STDIN_FILENO,
                events: libc::POLLIN,
                revents: 0,
            });
        }

        let mut sock_events: i16 = 0;
        if !sock_read_eof {
            sock_events |= libc::POLLIN;
        }
        if !pending_up.is_empty() {
            sock_events |= libc::POLLOUT;
        }
        if sock_events != 0 {
            pfds.push(libc::pollfd {
                fd: sock_fd,
                events: sock_events,
                revents: 0,
            });
        }

        if !pending_down.is_empty() {
            pfds.push(libc::pollfd {
                fd: STDOUT_FILENO,
                events: libc::POLLOUT,
                revents: 0,
            });
        }

        if pfds.is_empty() {
            log::warn!("rsync rsh poll: empty fd set (unexpected); spin yield");
            std::thread::yield_now();
            continue;
        }

        let pr = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, -1) };
        if pr < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(err);
        }

        let mut stdin_ready = false;
        let mut stdout_ready = false;
        let mut sock_in = false;
        let mut sock_out = false;
        let mut sock_err = false;
        for p in &pfds {
            if p.fd == STDIN_FILENO {
                stdin_ready = (p.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR)) != 0;
            } else if p.fd == STDOUT_FILENO {
                stdout_ready = (p.revents & libc::POLLOUT) != 0;
            } else if p.fd == sock_fd {
                sock_in = (p.revents & libc::POLLIN) != 0;
                sock_out = (p.revents & libc::POLLOUT) != 0;
                sock_err = (p.revents & libc::POLLERR) != 0;
            }
        }
        if sock_err {
            return Err(io::Error::other("rsync rsh bridge: socket POLLERR"));
        }

        // Order matters for rsync wire protocol: forward local stdin to the socket before draining
        // socket→stdout, so the remote peer can make progress before the parent reads our stdout.
        if stdin_ready {
            let n = unsafe {
                libc::read(
                    STDIN_FILENO,
                    buf.as_mut_ptr().cast::<libc::c_void>(),
                    buf.len(),
                )
            };
            if n < 0 {
                let err = io::Error::last_os_error();
                let errno = err.raw_os_error();
                if errno != Some(libc::EAGAIN) && errno != Some(libc::EWOULDBLOCK) {
                    return Err(err);
                }
            } else if n == 0 {
                stdin_eof = true;
                log::debug!("rsync rsh: stdin EOF");
            } else {
                pending_up.extend_from_slice(&buf[..n as usize]);
            }
        }

        if sock_out && !pending_up.is_empty() {
            match sock.write(&pending_up) {
                Ok(w) if w > 0 => {
                    pending_up.drain(..w);
                }
                Ok(_) => {}
                Err(e) if e.kind() == WouldBlock => {}
                Err(e) if e.kind() == BrokenPipe => {
                    log::debug!("rsync rsh: BrokenPipe writing socket (peer closed read side)");
                    pending_up.clear();
                    sock_read_eof = true;
                    shut_wr = true;
                }
                Err(e) => return Err(e),
            }
        }

        if sock_in {
            match sock.read(&mut buf) {
                Ok(0) => {
                    sock_read_eof = true;
                    log::debug!("rsync rsh: socket read EOF");
                }
                Ok(n) => pending_down.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == WouldBlock => {}
                Err(e) => return Err(e),
            }
        }

        if stdout_ready && !pending_down.is_empty() {
            let n = unsafe {
                libc::write(
                    STDOUT_FILENO,
                    pending_down.as_ptr().cast::<libc::c_void>(),
                    pending_down.len(),
                )
            };
            if n < 0 {
                let err = io::Error::last_os_error();
                let errno = err.raw_os_error();
                if errno != Some(libc::EAGAIN) && errno != Some(libc::EWOULDBLOCK) {
                    return Err(err);
                }
            } else if n > 0 {
                pending_down.drain(..n as usize);
            }
        }
    }

    Ok(())
}

#[cfg(not(unix))]
fn sync_byte_bridge(_addr: &str) -> Result<(), io::Error> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "rsync remote shell bridge requires Unix",
    ))
}

/// Spawn remote program for rsync's `RSYNC_RSH` (argv after host selection).
pub async fn run_rsync_rsh(
    config_path: Option<&Path>,
    args: Vec<OsString>,
) -> Result<ExitCode, io::Error> {
    let args_str: Vec<String> = args
        .iter()
        .map(|o| o.to_string_lossy().to_string())
        .collect();
    log::info!("run_rsync_rsh argc={}", args_str.len());
    if args_str.is_empty() {
        return Err(io_err("rsync rsh: missing authority/host argument"));
    }
    let authority = &args_str[0];
    let remote_argv: Vec<String> = args_str[1..].to_vec();
    if remote_argv.is_empty() {
        return Err(io_err("rsync rsh: missing remote program (expected rsync)"));
    }
    let base = resolve_connect_base(config_path, authority).map_err(|e| io_err(e.to_string()))?;
    let session =
        std::env::var(RSYNC_SESSION_ENV).unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());
    log::info!(
        "run_rsync_rsh authority={authority} session={session} remote_argc={}",
        remote_argv.len()
    );
    let argv_json =
        serde_json::to_string(&remote_argv).map_err(|e| io_err(format!("argv json: {e}")))?;
    let req = OpenRsyncSessionRequest { session, argv_json };
    let body = encode(&req);
    let resp_body = unary_proto(&base, "OpenRsyncSession", body)
        .await
        .map_err(io_err)?;
    let opened: OpenRsyncSessionResponse = decode(&resp_body).map_err(|e| io_err(e.to_string()))?;
    let addr = format!("{}:{}", opened.host, opened.port);
    log::info!("run_rsync_rsh connecting TCP {addr}");

    tokio::task::spawn_blocking(move || sync_byte_bridge(&addr))
        .await
        .map_err(|e| io_err(format!("join rsync bridge: {e}")))??;

    log::debug!("run_rsync_rsh byte bridge finished");
    Ok(ExitCode::SUCCESS)
}

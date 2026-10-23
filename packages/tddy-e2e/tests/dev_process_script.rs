//! Acceptance tests for `scripts/dev-process.sh` — the process handling `./web-dev` relies on, run
//! under whatever `bash` is first on a plain `PATH`: on macOS that is `/bin/bash` 3.2, where
//! `wait -n` does not exist and `fuser` has no `-k`. Both used to fail silently there: the launcher
//! fell straight through to its cleanup, and a stale daemon from an earlier run kept the port and
//! answered the readiness check in the new one's place.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

use tddy_e2e::dev_script_contract::shell_code_lines;

const SCRIPT: &str = "scripts/dev-process.sh";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

struct Outcome {
    succeeded: bool,
    stdout: String,
    stderr: String,
    elapsed: Duration,
}

/// Source the script under `./web-dev`'s shell options and run `body`, with only the system's own
/// tools on `PATH` — the `bash` a developer's shell hands `#!/usr/bin/env bash`.
fn run(body: &str) -> Outcome {
    run_in(&repo_root(), body)
}

/// [`run`] from `dir` — the checkout a per-checkout port is derived from.
fn run_in(dir: &Path, body: &str) -> Outcome {
    let script = format!(
        "set -euo pipefail\nsource \"{}\"\n{body}\n",
        repo_root().join(SCRIPT).display()
    );
    let started = Instant::now();
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .output()
        .expect("spawn bash");
    Outcome {
        succeeded: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        elapsed: started.elapsed(),
    }
}

/// A process a test started outside its own control — a detached group standing in for an earlier
/// run's daemon, or a long-lived child. Killed on drop, so a failing assertion does not leave it
/// running for minutes: its group first (`kill -- -<pid>`), then the pid itself.
struct Leftover(String);

impl Leftover {
    fn pid(&self) -> &str {
        &self.0
    }
}

impl Drop for Leftover {
    fn drop(&mut self) {
        let group = format!("-{}", self.0);
        let _ = Command::new("kill").args(["-KILL", "--", &group]).status();
        let _ = Command::new("kill").args(["-KILL", &self.0]).status();
    }
}

/// A port outside the kernel's ephemeral ranges (macOS 49152–65535, Linux 32768–60999) that is free
/// now. Nothing is handed such a port by the kernel, so it stays free between this check and the
/// script's — unlike a `:0` port dropped before use. Distinct per call within this process.
fn free_port_outside_ephemeral_range() -> u16 {
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let base = 20_000 + (std::process::id() % 5_000) as u16;
    loop {
        let offset = NEXT.fetch_add(1, Ordering::Relaxed);
        assert!(offset < 5_000, "no free port in 20000-30000");
        let port = base + offset;
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
}

/// `bash -n` must accept the script: `./web-dev` sources it under `set -euo pipefail`.
#[test]
fn dev_process_script_bash_syntax_is_valid() {
    tddy_e2e::dev_script_contract::verify_syntax(&repo_root().join(SCRIPT));
}

/// A port something already listens on is an error naming the port and what it is for — never a
/// kill, and never a launch that a stale process answers for.
#[test]
fn a_held_port_is_an_error_naming_it() {
    // Given
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    // When
    let outcome = run(&format!("dev_require_free_port {port} 'tddy-daemon'"));

    // Then
    assert!(
        !outcome.succeeded,
        "a held port must fail; stdout:\n{}",
        outcome.stdout
    );
    assert!(
        outcome.stderr.contains(&port.to_string()),
        "names the port: {}",
        outcome.stderr
    );
    assert!(
        outcome.stderr.contains("tddy-daemon"),
        "names what it is for: {}",
        outcome.stderr
    );
    assert!(
        outcome.stderr.contains("in use"),
        "says it is in use: {}",
        outcome.stderr
    );
    drop(listener);
}

/// The holder is left running: the check reports, it does not stop anything.
#[test]
fn a_held_port_is_left_to_its_holder() {
    // Given
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();

    // When
    let _ = run(&format!("dev_require_free_port {port} 'vite' || true"));

    // Then
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", port)).is_ok(),
        "the holder still accepts connections on {port}"
    );
    drop(listener);
}

/// A port nothing listens on passes.
#[test]
fn a_free_port_passes() {
    // Given
    let port = free_port_outside_ephemeral_range();

    // When
    let outcome = run(&format!("dev_require_free_port {port} 'vite'"));

    // Then
    assert!(
        outcome.succeeded,
        "a free port passes; stderr:\n{}",
        outcome.stderr
    );
}

/// `dev_wait_any` returns as soon as the first of its processes exits and names it — the job
/// `wait -n` does on bash ≥ 4.3, which macOS's bash 3.2 rejects as an invalid option.
#[test]
fn wait_any_returns_when_the_first_process_exits_and_names_it() {
    // Given one short-lived and one long-lived child, the long one's stdio detached so the shell's
    // output closes when it exits even if the body fails before killing it
    let body = r#"
sleep 0.3 & short=$!
sleep 30 </dev/null >/dev/null 2>&1 & long=$!
echo "long=$long"
dev_wait_any "$long" "$short"
echo "exited=$DEV_EXITED_PID short=$short"
kill "$long"
"#;

    // When
    let outcome = run(body);
    let _long = Leftover(outcome.value("long"));

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    let line = outcome
        .stdout
        .lines()
        .find(|l| l.starts_with("exited="))
        .expect("printed");
    let (exited, short) = line
        .strip_prefix("exited=")
        .and_then(|rest| rest.split_once(" short="))
        .expect("exited=<pid> short=<pid>");
    assert_eq!(exited, short, "names the process that exited");
    assert!(
        outcome.elapsed < Duration::from_secs(10),
        "returned promptly: {:?}",
        outcome.elapsed
    );
}

// ---------------------------------------------------------------------------
// Daemon readiness: stop waiting once the daemon is gone
// ---------------------------------------------------------------------------

/// A one-shot HTTP server answering every request 200, on a port it holds for the whole test.
fn serve_200() -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let Ok((mut conn, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 1024];
            let _ = conn.read(&mut buf);
            let _ = conn
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    (port, server)
}

/// A server that answers is ready: two 200s, a second apart, while its process runs.
#[test]
fn wait_http_ready_succeeds_once_the_server_answers() {
    // Given
    let (port, server) = serve_200();

    // When
    let outcome = run(&format!(
        "dev_wait_http_ready http://127.0.0.1:{port}/api/config \"$$\" 10 tddy-daemon"
    ));

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    server.join().expect("server thread");
}

/// A daemon that has exited — a bad config, a port lost in a race — never answers, so the wait
/// stops at once with an error saying so, instead of running out the whole timeout.
#[test]
fn wait_http_ready_fails_at_once_when_the_process_has_exited() {
    // Given a process that has already exited, and nothing answering
    let port = free_port_outside_ephemeral_range();

    // When
    let outcome = run(&format!(
        "true & gone=$!\nwait \"$gone\" || true\ndev_wait_http_ready http://127.0.0.1:{port}/api/config \"$gone\" 60 tddy-daemon"
    ));

    // Then
    assert!(
        !outcome.succeeded,
        "an exited process is never ready; stdout:\n{}",
        outcome.stdout
    );
    assert!(
        outcome.stderr.contains("tddy-daemon exited"),
        "says it exited: {}",
        outcome.stderr
    );
    assert!(
        outcome.elapsed < Duration::from_secs(10),
        "did not wait out the timeout: {:?}",
        outcome.elapsed
    );
}

/// A live process that never answers runs out the timeout, and the error names it and the URL.
#[test]
fn wait_http_ready_times_out_naming_the_url() {
    // Given
    let port = free_port_outside_ephemeral_range();

    // When
    let outcome = run(&format!(
        "dev_wait_http_ready http://127.0.0.1:{port}/api/config \"$$\" 2 tddy-daemon"
    ));

    // Then
    assert!(!outcome.succeeded);
    assert!(
        outcome
            .stderr
            .contains(&format!("http://127.0.0.1:{port}/api/config")),
        "{}",
        outcome.stderr
    );
    assert!(outcome.stderr.contains("within 2s"), "{}", outcome.stderr);
}

// ---------------------------------------------------------------------------
// ./web-dev's use of these functions (static: its code lines, comments removed)
// ---------------------------------------------------------------------------

/// `./web-dev`'s code, comments removed — a comment naming a command must not satisfy a test.
fn web_dev_code() -> Vec<String> {
    shell_code_lines(&fs::read_to_string(repo_root().join("web-dev")).expect("read web-dev"))
}

/// The index of the first code line `matches` accepts.
fn line_of(code: &[String], what: &str, matches: impl Fn(&str) -> bool) -> usize {
    code.iter()
        .position(|l| matches(l))
        .unwrap_or_else(|| panic!("web-dev has no code line {what}"))
}

fn daemon_start(code: &[String]) -> usize {
    line_of(code, "starting `./dev \"$DAEMON_BIN\"`", |l| {
        l.starts_with("./dev \"$DAEMON_BIN\"")
    })
}

#[test]
fn web_dev_sources_the_process_script() {
    line_of(&web_dev_code(), &format!("`source {SCRIPT}`"), |l| {
        l == format!("source {SCRIPT}")
    });
}

/// bash 3.2 rejects `wait -n` as an invalid option.
#[test]
fn web_dev_never_uses_wait_n() {
    let code = web_dev_code();
    assert!(!code.iter().any(|l| l.contains("wait -n")), "no `wait -n`");
}

/// macOS's `fuser` has no `-k`, and killing by port kills what is not ours.
#[test]
fn web_dev_never_uses_fuser() {
    let code = web_dev_code();
    assert!(!code.iter().any(|l| l.contains("fuser")), "no `fuser`");
}

/// It waits for its children with `dev_wait_any`, the bash 3.2 `wait -n`.
#[test]
fn web_dev_waits_for_its_children_with_dev_wait_any() {
    let code = web_dev_code();
    line_of(&code, "`dev_wait_any \"$daemon_pid\" \"$vite_pid\"`", |l| {
        l == "dev_wait_any \"$daemon_pid\" \"$vite_pid\""
    });
}

/// Vite fails on a taken port instead of moving off `VITE_URL`.
#[test]
fn web_dev_runs_vite_with_strict_port() {
    let code = web_dev_code();
    line_of(&code, "running `bunx vite … --strictPort`", |l| {
        l.contains("bunx vite ") && l.contains(" --strictPort ")
    });
}

/// Ports are resolved (and pinned ones checked) before anything is started.
#[test]
fn web_dev_resolves_ports_before_starting_the_daemon() {
    let code = web_dev_code();
    let resolve = line_of(&code, "`dev_resolve_ports`", |l| l == "dev_resolve_ports");
    assert!(
        resolve < daemon_start(&code),
        "ports resolved and checked first"
    );
}

/// Job control is on before the first background launch, so each child leads its own process group
/// and cleanup's `kill -- -<pid>` reaches it and its descendants.
#[test]
fn web_dev_enables_job_control_before_the_first_launch() {
    let code = web_dev_code();
    let job_control = line_of(&code, "`set -m`", |l| l == "set -m");
    assert!(
        job_control < daemon_start(&code),
        "job control on before the first background launch"
    );
}

/// The readiness wait is told the daemon's pid, so a daemon that exits ends it at once.
#[test]
fn web_dev_waits_for_daemon_readiness_on_its_pid() {
    let code = web_dev_code();
    let wait = line_of(
        &code,
        "calling `dev_wait_http_ready … \"$daemon_pid\"`",
        |l| {
            l.contains("dev_wait_http_ready ")
                && l.contains("/api/config")
                && l.contains("\"$daemon_pid\"")
        },
    );
    assert!(
        daemon_start(&code) < wait,
        "readiness is awaited after the daemon starts"
    );
}

/// Only processes still alive are escalated to KILL: a pid that exited after TERM may since have been
/// handed to another process.
#[test]
fn web_dev_stop_children_kills_only_what_is_still_alive() {
    let code = web_dev_code();
    let start = line_of(&code, "`stop_children() {`", |l| l == "stop_children() {");
    let body: Vec<&String> = code[start + 1..]
        .iter()
        .take_while(|l| l.as_str() != "}")
        .collect();
    let kills: Vec<&&String> = body.iter().filter(|l| l.contains("kill -KILL")).collect();
    assert!(
        !kills.is_empty(),
        "stop_children escalates to KILL: {body:#?}"
    );
    assert!(
        kills
            .iter()
            .all(|l| l.contains("kill -0 ") && l.find("kill -0 ") < l.find("kill -KILL")),
        "every KILL is guarded by a liveness check on the same line: {kills:#?}"
    );
}

// ---------------------------------------------------------------------------
// Port resolution: ephemeral unless pinned
// ---------------------------------------------------------------------------

/// Run `dev_resolve_ports` with `env` and print what it resolved.
fn resolve(env: &[(&str, &str)]) -> Outcome {
    let exports: String = env
        .iter()
        .map(|(k, v)| format!("export {k}='{v}'\n"))
        .collect();
    run(&format!(
        "{exports}HEADLESS=false\ndev_resolve_ports\necho \"DAEMON_PORT=$DAEMON_PORT\"\necho \"VITE_PORT=$VITE_PORT\"\necho \"VITE_URL=$VITE_URL\""
    ))
}

impl Outcome {
    fn value(&self, name: &str) -> String {
        let prefix = format!("{name}=");
        self.stdout
            .lines()
            .find_map(|l| l.strip_prefix(&prefix))
            .unwrap_or_else(|| {
                panic!(
                    "{name} not printed; stdout:\n{}\nstderr:\n{}",
                    self.stdout, self.stderr
                )
            })
            .to_owned()
    }

    fn port(&self, name: &str) -> u16 {
        self.value(name)
            .parse()
            .unwrap_or_else(|_| panic!("{name} is a port: {}", self.value(name)))
    }
}

/// [`resolve`] from `dir`, the checkout unpinned ports are derived from.
fn resolve_in(dir: &Path, env: &[(&str, &str)]) -> Outcome {
    let exports: String = env
        .iter()
        .map(|(k, v)| format!("export {k}='{v}'\n"))
        .collect();
    run_in(
        dir,
        &format!(
            "{exports}HEADLESS=false\ndev_resolve_ports\necho \"DAEMON_PORT=$DAEMON_PORT\"\necho \"VITE_PORT=$VITE_PORT\"\necho \"VITE_URL=$VITE_URL\""
        ),
    )
}

/// Nothing pinned → both ports are in the ephemeral range and distinct, and the browser URL follows
/// Vite's. (That a held port is never chosen is shown by the held-port tests below.)
#[test]
fn unpinned_ports_are_ephemeral_and_distinct() {
    // When
    let outcome = resolve(&[]);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    let (daemon, vite) = (outcome.port("DAEMON_PORT"), outcome.port("VITE_PORT"));
    for port in [daemon, vite] {
        assert!(
            (49152..=65535).contains(&port),
            "{port} is in the ephemeral range"
        );
    }
    assert_ne!(daemon, vite);
    assert_eq!(
        outcome.value("VITE_URL"),
        format!("http://127.0.0.1:{vite}")
    );
}

/// The same checkout resolves the same unpinned ports run after run, so the browser origin — and
/// the session token and host-key pins the browser keeps per origin — survives a restart.
#[test]
fn unpinned_ports_are_stable_per_checkout() {
    // Given
    let checkout = tempfile::TempDir::new().expect("tempdir");

    // When
    let first = resolve_in(checkout.path(), &[]);
    let second = resolve_in(checkout.path(), &[]);

    // Then
    assert!(
        first.succeeded && second.succeeded,
        "stderr:\n{}\n{}",
        first.stderr,
        second.stderr
    );
    assert_eq!(first.port("VITE_PORT"), second.port("VITE_PORT"));
    assert_eq!(first.port("DAEMON_PORT"), second.port("DAEMON_PORT"));
}

/// The checkout's Vite port held by something else → another free port for this run, and a notice
/// saying so (the browser origin changes with it). The holder is not touched.
#[test]
fn a_held_checkout_vite_port_moves_to_another_free_port_with_a_notice() {
    // Given the checkout's own Vite port, held
    let checkout = tempfile::TempDir::new().expect("tempdir");
    let preferred = resolve_in(checkout.path(), &[]).port("VITE_PORT");
    let _holder =
        TcpListener::bind(("127.0.0.1", preferred)).expect("hold the checkout's Vite port");

    // When
    let outcome = resolve_in(checkout.path(), &[]);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    let vite = outcome.port("VITE_PORT");
    assert_ne!(vite, preferred, "a held port is not chosen");
    assert!(
        (49152..=65535).contains(&vite),
        "{vite} is in the ephemeral range"
    );
    assert_ne!(vite, outcome.port("DAEMON_PORT"));
    assert!(
        outcome.stderr.contains(&preferred.to_string())
            && outcome.stderr.contains(&vite.to_string()),
        "the notice names both ports: {}",
        outcome.stderr
    );
}

/// The same for the daemon's port.
#[test]
fn a_held_checkout_daemon_port_moves_to_another_free_port_with_a_notice() {
    // Given
    let checkout = tempfile::TempDir::new().expect("tempdir");
    let preferred = resolve_in(checkout.path(), &[]).port("DAEMON_PORT");
    let _holder =
        TcpListener::bind(("127.0.0.1", preferred)).expect("hold the checkout's daemon port");

    // When
    let outcome = resolve_in(checkout.path(), &[]);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    let daemon = outcome.port("DAEMON_PORT");
    assert_ne!(daemon, preferred, "a held port is not chosen");
    assert!(
        outcome.stderr.contains(&preferred.to_string())
            && outcome.stderr.contains(&daemon.to_string()),
        "the notice names both ports: {}",
        outcome.stderr
    );
}

/// A pinned daemon port is kept.
#[test]
fn a_pinned_daemon_port_is_kept() {
    // Given
    let port = free_port_outside_ephemeral_range().to_string();

    // When
    let outcome = resolve(&[("DAEMON_PORT", &port)]);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    assert_eq!(outcome.value("DAEMON_PORT"), port);
}

/// A `VITE_URL` naming a port pins Vite to it — that URL is the OAuth redirect base, so Vite must
/// serve exactly there.
#[test]
fn a_port_in_vite_url_pins_vite() {
    // Given
    let port = free_port_outside_ephemeral_range();

    // When
    let outcome = resolve(&[("VITE_URL", &format!("http://localhost:{port}/"))]);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    assert_eq!(outcome.port("VITE_PORT"), port);
    assert_eq!(
        outcome.value("VITE_URL"),
        format!("http://localhost:{port}")
    );
}

/// `VITE_PORT` alone pins Vite and the URL follows it.
#[test]
fn vite_port_alone_pins_vite_and_the_url_follows() {
    // Given
    let port = free_port_outside_ephemeral_range();

    // When
    let outcome = resolve(&[("VITE_PORT", &port.to_string())]);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    assert_eq!(
        outcome.value("VITE_URL"),
        format!("http://127.0.0.1:{port}")
    );
}

/// `VITE_PORT` and a `VITE_URL` port that disagree cannot both be served: an error naming both.
#[test]
fn a_vite_port_that_disagrees_with_vite_url_is_an_error() {
    // When
    let outcome = resolve(&[
        ("VITE_PORT", "50001"),
        ("VITE_URL", "http://localhost:50002"),
    ]);

    // Then
    assert!(!outcome.succeeded);
    assert!(
        outcome.stderr.contains("50001") && outcome.stderr.contains("50002"),
        "{}",
        outcome.stderr
    );
}

/// A pinned port that is held is the same named error as before — pinned means it cannot move.
#[test]
fn a_pinned_port_that_is_held_is_an_error() {
    // Given
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    // When
    let outcome = resolve(&[("VITE_URL", &format!("http://localhost:{port}"))]);

    // Then
    assert!(!outcome.succeeded);
    assert!(outcome.stderr.contains("in use"), "{}", outcome.stderr);
    drop(listener);
}

/// The daemon listens on this run's port: `web-dev` writes it over `web_port:` in the temp config.
#[test]
fn web_dev_writes_the_daemon_port_into_its_config() {
    let code = web_dev_code();
    line_of(&code, "rewriting `web_port:` to `${DAEMON_PORT}`", |l| {
        l.starts_with("-e ") && l.contains("web_port:") && l.contains("${DAEMON_PORT}")
    });
}

/// The terminal closing stops the children too, not only Ctrl+C.
#[test]
fn web_dev_traps_hangup() {
    let code = web_dev_code();
    line_of(&code, "`trap … HUP`", |l| {
        l.starts_with("trap ") && l.ends_with(" HUP")
    });
}

// ---------------------------------------------------------------------------
// Run record: a later run stops what an earlier one left behind — and only that
// ---------------------------------------------------------------------------

/// A scratch dir holding a run record and a `docker` stub that logs its calls.
struct RunRecord {
    dir: tempfile::TempDir,
}

impl RunRecord {
    fn new() -> Self {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let stub = dir.path().join("docker");
        fs::write(
            &stub,
            "#!/usr/bin/env bash\necho \"docker $*\" >> \"$STUB_LOG\"\n",
        )
        .unwrap();
        fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn file(&self) -> String {
        self.dir.path().join("web-dev.run").display().to_string()
    }

    fn docker_calls(&self) -> String {
        fs::read_to_string(self.dir.path().join("docker.log")).unwrap_or_default()
    }

    /// Run `body` with the record's functions sourced, its stub first on `PATH` and `$RUN` naming
    /// the record file.
    fn run(&self, body: &str) -> Outcome {
        let script = format!(
            "set -euo pipefail\nsource \"{}\"\nRUN=\"{}\"\n{body}\n",
            repo_root().join(SCRIPT).display(),
            self.file()
        );
        let started = Instant::now();
        let out = Command::new("bash")
            .arg("-c")
            .arg(script)
            .env_clear()
            .env(
                "PATH",
                format!(
                    "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                    self.dir.path().display()
                ),
            )
            .env("STUB_LOG", self.dir.path().join("docker.log"))
            .output()
            .expect("spawn bash");
        Outcome {
            succeeded: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            elapsed: started.elapsed(),
        }
    }
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// A detached process group standing in for an earlier run's daemon: it outlives the shell that
/// recorded it, the way a leftover does after `kill -9` of ./web-dev.
/// Its stdio is detached so the test's output pipes close when the recording shell exits.
const SPAWN_LEFTOVER: &str =
    r#"set -m; sleep 300 </dev/null >/dev/null 2>&1 & leftover=$!; set +m; disown "$leftover""#;

/// [`SPAWN_LEFTOVER`] whose process ignores TERM (an ignored signal survives `exec`), so only the
/// KILL escalation stops it.
const SPAWN_TERM_IGNORING_LEFTOVER: &str = r#"set -m; bash -c 'trap "" TERM; exec sleep 300' </dev/null >/dev/null 2>&1 & leftover=$!; set +m; disown "$leftover""#;

/// A recorded group from a run that is gone is stopped by the next run.
#[test]
fn a_recorded_group_left_by_a_dead_run_is_stopped() {
    // Given an earlier run (a shell that has since exited) that recorded one process group
    let record = RunRecord::new();
    let earlier = record.run(&format!(
        "{SPAWN_LEFTOVER}\necho \"leftover=$leftover\"\ndev_run_record_init \"$RUN\"\ndev_run_record_group \"$RUN\" \"$leftover\""
    ));
    let leftover = Leftover(earlier.value("leftover"));
    assert!(earlier.succeeded, "stderr:\n{}", earlier.stderr);
    assert!(alive(leftover.pid()), "the leftover outlived its run");

    // When the next run reaps
    let next = record.run("dev_reap_previous_run \"$RUN\"");

    // Then
    assert!(next.succeeded, "stderr:\n{}", next.stderr);
    assert!(
        !alive(leftover.pid()),
        "leftover {} stopped",
        leftover.pid()
    );
    assert!(
        next.stderr.contains(leftover.pid()),
        "says what it stopped: {}",
        next.stderr
    );
    assert!(
        !Path::new(&record.file()).exists(),
        "the old record is gone"
    );
}

/// A leftover that ignores TERM is escalated to KILL once the grace period is over — still only
/// because its recorded start time matches.
#[test]
fn a_recorded_group_that_ignores_term_is_killed() {
    // Given an earlier run's group whose process ignores TERM
    let record = RunRecord::new();
    let earlier = record.run(&format!(
        "{SPAWN_TERM_IGNORING_LEFTOVER}\necho \"leftover=$leftover\"\ndev_run_record_init \"$RUN\"\ndev_run_record_group \"$RUN\" \"$leftover\""
    ));
    let leftover = Leftover(earlier.value("leftover"));
    assert!(earlier.succeeded, "stderr:\n{}", earlier.stderr);
    assert!(alive(leftover.pid()), "the leftover outlived its run");

    // When the next run reaps
    let next = record.run("dev_reap_previous_run \"$RUN\"");

    // Then
    assert!(next.succeeded, "stderr:\n{}", next.stderr);
    assert!(
        !alive(leftover.pid()),
        "leftover {} killed after ignoring TERM",
        leftover.pid()
    );
}

/// A recorded pid now held by a different process (started at another time) is not ours: untouched.
#[test]
fn a_recorded_pid_reused_by_another_process_is_left_alone() {
    // Given a record naming a live process with a start time that is not its own
    let record = RunRecord::new();
    let spawned = record.run(&format!(
        "{SPAWN_LEFTOVER}\necho \"leftover=$leftover\"\nprintf 'owner 1 Thu Jan  1 00:00:00 1970\\ngroup %s Thu Jan  1 00:00:00 1970\\n' \"$leftover\" > \"$RUN\""
    ));
    let other = Leftover(spawned.value("leftover"));
    assert!(spawned.succeeded, "stderr:\n{}", spawned.stderr);

    // When
    let next = record.run("dev_reap_previous_run \"$RUN\"");

    // Then
    assert!(next.succeeded, "stderr:\n{}", next.stderr);
    assert!(
        alive(other.pid()),
        "a process that is not the recorded one is not killed"
    );
}

/// The recorded run's own LiveKit container is removed; nothing else is.
#[test]
fn a_recorded_container_is_removed() {
    // Given
    let record = RunRecord::new();
    let earlier = record.run(
        "dev_run_record_init \"$RUN\"\ndev_run_record_container \"$RUN\" tddy-web-dev-livekit-4242\ndev_run_record_container \"$RUN\" someone-elses-livekit",
    );
    assert!(earlier.succeeded, "stderr:\n{}", earlier.stderr);
    // The recording shell has exited, so the record's owner is gone.

    // When
    let next = record.run("dev_reap_previous_run \"$RUN\"");

    // Then
    assert!(next.succeeded, "stderr:\n{}", next.stderr);
    let calls = record.docker_calls();
    assert!(calls.contains("rm -f tddy-web-dev-livekit-4242"), "{calls}");
    assert!(
        !calls.contains("someone-elses-livekit"),
        "a container not named as web-dev's is never touched: {calls}"
    );
}

/// A record whose owner is still running is a live run, not a leftover: an error, nothing stopped.
#[test]
fn a_record_whose_owner_is_alive_is_a_live_run_and_is_not_touched() {
    // Given a run that is still going (this shell owns the record and a group)
    let record = RunRecord::new();

    // When the same record is reaped while its owner is alive
    let outcome = record.run(&format!(
        "{SPAWN_LEFTOVER}\necho \"leftover=$leftover\"\ndev_run_record_init \"$RUN\"\ndev_run_record_group \"$RUN\" \"$leftover\"\ndev_reap_previous_run \"$RUN\""
    ));
    let leftover = Leftover(outcome.value("leftover"));

    // Then
    assert!(
        !outcome.succeeded,
        "a live run must refuse; stdout:\n{}",
        outcome.stdout
    );
    assert!(
        outcome.stderr.contains("already running"),
        "{}",
        outcome.stderr
    );
    assert!(
        alive(leftover.pid()),
        "the live run's processes are untouched"
    );
}

/// Leftovers are reaped before ports are chosen, so a leftover's port is free again for this run.
#[test]
fn web_dev_reaps_before_resolving_ports() {
    let code = web_dev_code();
    let reap = line_of(&code, "`dev_reap_previous_run \"$RUN_RECORD\"`", |l| {
        l == "dev_reap_previous_run \"$RUN_RECORD\""
    });
    let resolve = line_of(&code, "`dev_resolve_ports`", |l| l == "dev_resolve_ports");
    assert!(reap < resolve, "reaps before choosing ports");
}

#[test]
fn web_dev_starts_its_run_record_before_the_daemon() {
    let code = web_dev_code();
    let init = line_of(&code, "`dev_run_record_init \"$RUN_RECORD\"`", |l| {
        l == "dev_run_record_init \"$RUN_RECORD\""
    });
    assert!(init < daemon_start(&code));
}

#[test]
fn web_dev_records_the_daemon_group() {
    let code = web_dev_code();
    line_of(&code, "recording the daemon's group", |l| {
        l == "dev_run_record_group \"$RUN_RECORD\" \"$daemon_pid\""
    });
}

#[test]
fn web_dev_records_the_vite_group() {
    let code = web_dev_code();
    line_of(&code, "recording Vite's group", |l| {
        l == "dev_run_record_group \"$RUN_RECORD\" \"$vite_pid\""
    });
}

#[test]
fn web_dev_records_its_livekit_container() {
    let code = web_dev_code();
    line_of(&code, "recording the LiveKit container", |l| {
        l.contains("dev_run_record_container \"$RUN_RECORD\" \"$DEV_LIVEKIT_CONTAINER\"")
    });
}

/// The record is removed on exit, once this run owns it.
#[test]
fn web_dev_on_exit_removes_its_run_record() {
    let code = web_dev_code();
    let start = line_of(&code, "`on_exit() {`", |l| l == "on_exit() {");
    let body: Vec<&String> = code[start + 1..]
        .iter()
        .take_while(|l| l.as_str() != "}")
        .collect();
    assert!(
        body.iter().any(|l| l.contains("rm -f \"$RUN_RECORD\"")),
        "on_exit removes the run record: {body:#?}"
    );
}

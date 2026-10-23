//! Acceptance tests for `scripts/dev-process.sh` — the process handling `./web-dev` relies on, run
//! under whatever `bash` is first on a plain `PATH`: on macOS that is `/bin/bash` 3.2, where
//! `wait -n` does not exist and `fuser` has no `-k`. Both used to fail silently there: the launcher
//! fell straight through to its cleanup, and a stale daemon from an earlier run kept the port and
//! answered the readiness check in the new one's place.

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

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
    let script = format!(
        "set -euo pipefail\nsource \"{}\"\n{body}\n",
        repo_root().join(SCRIPT).display()
    );
    let started = Instant::now();
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
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
    assert!(!outcome.succeeded, "a held port must fail; stdout:\n{}", outcome.stdout);
    assert!(outcome.stderr.contains(&port.to_string()), "names the port: {}", outcome.stderr);
    assert!(outcome.stderr.contains("tddy-daemon"), "names what it is for: {}", outcome.stderr);
    assert!(outcome.stderr.contains("in use"), "says it is in use: {}", outcome.stderr);
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
    // Given a port that was free a moment ago
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port();

    // When
    let outcome = run(&format!("dev_require_free_port {port} 'vite'"));

    // Then
    assert!(outcome.succeeded, "a free port passes; stderr:\n{}", outcome.stderr);
}

/// `dev_wait_any` returns as soon as the first of its processes exits and names it — the job
/// `wait -n` does on bash ≥ 4.3, which macOS's bash 3.2 rejects as an invalid option.
#[test]
fn wait_any_returns_when_the_first_process_exits_and_names_it() {
    // Given one short-lived and one long-lived child
    let body = r#"
sleep 0.3 & short=$!
sleep 30 & long=$!
dev_wait_any "$long" "$short"
echo "exited=$DEV_EXITED_PID short=$short"
kill "$long"
"#;

    // When
    let outcome = run(body);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    let line = outcome.stdout.lines().find(|l| l.starts_with("exited=")).expect("printed");
    let (exited, short) = line
        .strip_prefix("exited=")
        .and_then(|rest| rest.split_once(" short="))
        .expect("exited=<pid> short=<pid>");
    assert_eq!(exited, short, "names the process that exited");
    assert!(outcome.elapsed < Duration::from_secs(10), "returned promptly: {:?}", outcome.elapsed);
}

/// `./web-dev` must keep to what works on bash 3.2 and must not kill by port.
#[test]
fn web_dev_uses_portable_waiting_and_never_kills_by_port() {
    // Given
    let web_dev = fs::read_to_string(repo_root().join("web-dev")).expect("read web-dev");
    let code: Vec<&str> = web_dev
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    let code = code.join("\n");

    // Then
    assert!(code.contains(&format!("source {SCRIPT}")), "web-dev sources {SCRIPT}");
    assert!(!code.contains("wait -n"), "no `wait -n`: bash 3.2 rejects it");
    assert!(!code.contains("fuser"), "no `fuser`: macOS's has no -k, and killing by port kills what is not ours");
    assert!(code.contains("dev_wait_any"), "waits with dev_wait_any");
    assert!(code.contains("--strictPort"), "Vite fails on a taken port instead of moving off VITE_URL");
}

/// Ports are resolved (and pinned ones checked) before anything is started, and job control is on before the first
/// background launch — so each child leads its own process group and cleanup's `kill -- -<pid>`
/// reaches it and its descendants.
#[test]
fn web_dev_checks_ports_and_enables_job_control_before_launching() {
    // Given
    let web_dev = fs::read_to_string(repo_root().join("web-dev")).expect("read web-dev");
    let pos = |needle: &str| {
        web_dev
            .find(needle)
            .unwrap_or_else(|| panic!("web-dev must contain {needle:?}"))
    };

    // Then
    let daemon_start = pos("./dev \"$DAEMON_BIN\"");
    assert!(pos("\ndev_resolve_ports\n") < daemon_start, "ports resolved and checked first");
    assert!(pos("\nset -m\n") < daemon_start, "job control on before the first background launch");
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
            .unwrap_or_else(|| panic!("{name} not printed; stdout:\n{}\nstderr:\n{}", self.stdout, self.stderr))
            .to_owned()
    }

    fn port(&self, name: &str) -> u16 {
        self.value(name).parse().unwrap_or_else(|_| panic!("{name} is a port: {}", self.value(name)))
    }
}

/// Nothing pinned → both ports are ephemeral, distinct and free, and the browser URL follows Vite's.
#[test]
fn unpinned_ports_are_ephemeral_free_and_distinct() {
    // When
    let outcome = resolve(&[]);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    let (daemon, vite) = (outcome.port("DAEMON_PORT"), outcome.port("VITE_PORT"));
    for port in [daemon, vite] {
        assert!((49152..=65535).contains(&port), "{port} is in the ephemeral range");
        assert!(TcpListener::bind(("127.0.0.1", port)).is_ok(), "{port} is free");
    }
    assert_ne!(daemon, vite);
    assert_eq!(outcome.value("VITE_URL"), format!("http://127.0.0.1:{vite}"));
}

/// A pinned daemon port is kept.
#[test]
fn a_pinned_daemon_port_is_kept() {
    // Given
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port().to_string();

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
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

    // When
    let outcome = resolve(&[("VITE_URL", &format!("http://localhost:{port}/"))]);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    assert_eq!(outcome.port("VITE_PORT"), port);
    assert_eq!(outcome.value("VITE_URL"), format!("http://localhost:{port}"));
}

/// `VITE_PORT` alone pins Vite and the URL follows it.
#[test]
fn vite_port_alone_pins_vite_and_the_url_follows() {
    // Given
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

    // When
    let outcome = resolve(&[("VITE_PORT", &port.to_string())]);

    // Then
    assert!(outcome.succeeded, "stderr:\n{}", outcome.stderr);
    assert_eq!(outcome.value("VITE_URL"), format!("http://127.0.0.1:{port}"));
}

/// `VITE_PORT` and a `VITE_URL` port that disagree cannot both be served: an error naming both.
#[test]
fn a_vite_port_that_disagrees_with_vite_url_is_an_error() {
    // When
    let outcome = resolve(&[("VITE_PORT", "50001"), ("VITE_URL", "http://localhost:50002")]);

    // Then
    assert!(!outcome.succeeded);
    assert!(outcome.stderr.contains("50001") && outcome.stderr.contains("50002"), "{}", outcome.stderr);
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

/// `./web-dev` resolves its ports this way, writes the daemon's into the config it hands the daemon,
/// and stops its children when its terminal closes, not only on Ctrl+C.
#[test]
fn web_dev_resolves_ports_writes_the_daemon_port_and_closes_on_hangup() {
    // Given
    let web_dev = fs::read_to_string(repo_root().join("web-dev")).expect("read web-dev");

    // Then
    assert!(web_dev.contains("\ndev_resolve_ports\n"), "web-dev calls dev_resolve_ports");
    assert!(web_dev.contains("web_port:"), "web-dev writes web_port into the daemon's config");
    assert!(
        web_dev.lines().any(|l| l.trim_start().starts_with("trap ") && l.trim_end().ends_with(" HUP")),
        "web-dev traps HUP"
    );
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
        fs::write(&stub, "#!/usr/bin/env bash\necho \"docker $*\" >> \"$STUB_LOG\"\n").unwrap();
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
            .env("PATH", format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", self.dir.path().display()))
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
    Command::new("kill").args(["-0", pid]).status().map(|s| s.success()).unwrap_or(false)
}

/// A detached process group standing in for an earlier run's daemon: it outlives the shell that
/// recorded it, the way a leftover does after `kill -9` of ./web-dev.
/// Its stdio is detached so the test's output pipes close when the recording shell exits.
const SPAWN_LEFTOVER: &str =
    r#"set -m; sleep 300 </dev/null >/dev/null 2>&1 & leftover=$!; set +m; disown "$leftover""#;

/// A recorded group from a run that is gone is stopped by the next run.
#[test]
fn a_recorded_group_left_by_a_dead_run_is_stopped() {
    // Given an earlier run (a shell that has since exited) that recorded one process group
    let record = RunRecord::new();
    let earlier = record.run(&format!(
        "{SPAWN_LEFTOVER}\ndev_run_record_init \"$RUN\"\ndev_run_record_group \"$RUN\" \"$leftover\"\necho \"leftover=$leftover\""
    ));
    assert!(earlier.succeeded, "stderr:\n{}", earlier.stderr);
    let leftover = earlier.value("leftover");
    assert!(alive(&leftover), "the leftover outlived its run");

    // When the next run reaps
    let next = record.run("dev_reap_previous_run \"$RUN\"");

    // Then
    assert!(next.succeeded, "stderr:\n{}", next.stderr);
    assert!(!alive(&leftover), "leftover {leftover} stopped");
    assert!(next.stderr.contains(&leftover), "says what it stopped: {}", next.stderr);
    assert!(!std::path::Path::new(&record.file()).exists(), "the old record is gone");
}

/// A recorded pid now held by a different process (started at another time) is not ours: untouched.
#[test]
fn a_recorded_pid_reused_by_another_process_is_left_alone() {
    // Given a record naming a live process with a start time that is not its own
    let record = RunRecord::new();
    let spawned = record.run(&format!(
        "{SPAWN_LEFTOVER}\nprintf 'owner 1 Thu Jan  1 00:00:00 1970\\ngroup %s Thu Jan  1 00:00:00 1970\\n' \"$leftover\" > \"$RUN\"\necho \"leftover=$leftover\""
    ));
    let other = spawned.value("leftover");

    // When
    let next = record.run("dev_reap_previous_run \"$RUN\"");

    // Then
    assert!(next.succeeded, "stderr:\n{}", next.stderr);
    assert!(alive(&other), "a process that is not the recorded one is not killed");
    let _ = Command::new("kill").arg(&other).status();
}

/// The recorded run's own LiveKit container is removed; nothing else is.
#[test]
fn a_recorded_container_is_removed() {
    // Given
    let record = RunRecord::new();
    record.run("dev_run_record_init \"$RUN\"\ndev_run_record_container \"$RUN\" tddy-web-dev-livekit-4242");
    // The recording shell has exited, so the record's owner is gone.

    // When
    let next = record.run("dev_reap_previous_run \"$RUN\"");

    // Then
    assert!(next.succeeded, "stderr:\n{}", next.stderr);
    assert!(record.docker_calls().contains("rm -f tddy-web-dev-livekit-4242"), "{}", record.docker_calls());
}

/// A record whose owner is still running is a live run, not a leftover: an error, nothing stopped.
#[test]
fn a_record_whose_owner_is_alive_is_a_live_run_and_is_not_touched() {
    // Given a run that is still going (this shell owns the record and a group)
    let record = RunRecord::new();

    // When the same record is reaped while its owner is alive
    let outcome = record.run(&format!(
        "{SPAWN_LEFTOVER}\ndev_run_record_init \"$RUN\"\ndev_run_record_group \"$RUN\" \"$leftover\"\necho \"leftover=$leftover\"\ndev_reap_previous_run \"$RUN\""
    ));

    // Then
    assert!(!outcome.succeeded, "a live run must refuse; stdout:\n{}", outcome.stdout);
    assert!(outcome.stderr.contains("already running"), "{}", outcome.stderr);
    let leftover = outcome.value("leftover");
    assert!(alive(&leftover), "the live run's processes are untouched");
    let _ = Command::new("kill").arg(&leftover).status();
}

/// `./web-dev` reaps before resolving ports, records each group and its container, and drops its
/// record on exit.
#[test]
fn web_dev_reaps_records_and_clears_its_run_record() {
    // Given
    let web_dev = fs::read_to_string(repo_root().join("web-dev")).expect("read web-dev");
    let pos = |needle: &str| {
        web_dev
            .find(needle)
            .unwrap_or_else(|| panic!("web-dev must contain {needle:?}"))
    };

    // Then
    assert!(pos("dev_reap_previous_run") < pos("\ndev_resolve_ports\n"), "reaps before choosing ports");
    assert!(pos("dev_run_record_init") < pos("./dev \"$DAEMON_BIN\""));
    assert!(web_dev.contains("dev_run_record_group \"$RUN_RECORD\" \"$daemon_pid\""));
    assert!(web_dev.contains("dev_run_record_group \"$RUN_RECORD\" \"$vite_pid\""));
    assert!(web_dev.contains("dev_run_record_container"));
    let on_exit = web_dev.split("on_exit() {").nth(1).and_then(|b| b.split("\n}").next()).expect("on_exit");
    assert!(on_exit.contains("RUN_RECORD"), "on_exit removes the run record: {on_exit}");
}

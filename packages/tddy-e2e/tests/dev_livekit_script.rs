//! Acceptance tests for `scripts/dev-livekit.sh` — the LiveKit server `./web-dev` runs against.
//!
//! Docker is the primary path: with no LiveKit credentials in the environment (`.env` included),
//! `dev_livekit_up` starts a container of its own and `dev_livekit_down` removes it. Only when
//! `.env` names a complete external server (URL, key and secret) is that server reused, and then
//! no container is started. A LiveKit the script did not start is never stopped or killed.
//!
//! These run the real script with `docker` and `curl` replaced by stubs on `PATH`, so nothing
//! touches a real Docker daemon or the network. The stubs record every call they receive.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use pretty_assertions::assert_eq;
use tempfile::TempDir;

const SCRIPT: &str = "scripts/dev-livekit.sh";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Records each invocation as one line in `$STUB_LOG`. The first `$STUB_DOCKER_BUSY_RUNS` `run`
/// calls fail the way Docker does when a host port is already allocated.
const DOCKER_STUB: &str = r#"#!/usr/bin/env bash
echo "docker $*" >> "$STUB_LOG"
case "$1" in
  info)
    [ "${STUB_DOCKER_DOWN:-0}" = "1" ] && { echo "Cannot connect to the Docker daemon" >&2; exit 1; }
    exit 0 ;;
  run)
    runs=$(( $(cat "$STUB_LOG.runs" 2>/dev/null || echo 0) + 1 ))
    echo "$runs" > "$STUB_LOG.runs"
    if [ "$runs" -le "${STUB_DOCKER_BUSY_RUNS:-0}" ]; then
      echo "Bind for 0.0.0.0:49999 failed: port is already allocated" >&2
      exit 125
    fi
    echo "c0ffee"
    exit 0 ;;
  logs) echo "livekit-server: stub log line"; exit 0 ;;
  *) exit 0 ;;
esac
"#;

/// Answers every request with `$STUB_CURL_CODE` (default 200), the way `curl -w '%{http_code}'`
/// does — `000` is what curl prints when nothing answers.
const CURL_STUB: &str = r#"#!/usr/bin/env bash
echo "curl $*" >> "$STUB_LOG"
printf '%s' "${STUB_CURL_CODE:-200}"
"#;

struct Launcher {
    dir: TempDir,
    env: Vec<(&'static str, String)>,
}

struct Outcome {
    succeeded: bool,
    stdout: String,
    stderr: String,
    calls: Vec<String>,
}

impl Launcher {
    fn with_no_livekit_in_env() -> Self {
        let dir = TempDir::new().expect("tempdir");
        for (name, body) in [("docker", DOCKER_STUB), ("curl", CURL_STUB)] {
            let path = dir.path().join(name);
            fs::write(&path, body).expect("write stub");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod stub");
        }
        Self {
            dir,
            env: vec![("DEV_LIVEKIT_READY_TIMEOUT_S", "2".into())],
        }
    }

    fn with(mut self, name: &'static str, value: &str) -> Self {
        self.env.push((name, value.into()));
        self
    }

    fn with_external_livekit(self, url: &str) -> Self {
        self.with("LIVEKIT_URL", url)
            .with("LIVEKIT_API_KEY", "external-key")
            .with("LIVEKIT_API_SECRET", "external-secret")
    }

    fn with_first_port_blocks_taken(self, count: u32) -> Self {
        self.with("STUB_DOCKER_BUSY_RUNS", &count.to_string())
    }

    fn with_docker_not_running(self) -> Self {
        self.with("STUB_DOCKER_DOWN", "1")
    }

    fn with_livekit_never_answering(self) -> Self {
        self.with("STUB_CURL_CODE", "000")
    }

    fn log_path(&self) -> PathBuf {
        self.dir.path().join("calls.log")
    }

    /// Source the script under the launcher's own shell options, the way `./web-dev` does, register
    /// `dev_livekit_down` on EXIT, bring LiveKit up, print what it exported, then run `after`.
    fn run(&self, after: &str) -> Outcome {
        let body = format!(
            r#"set -euo pipefail
source "{script}"
trap dev_livekit_down EXIT
dev_livekit_up
echo "LIVEKIT_URL=${{LIVEKIT_URL:-}}"
echo "LIVEKIT_PUBLIC_URL=${{LIVEKIT_PUBLIC_URL:-}}"
echo "LIVEKIT_API_KEY=${{LIVEKIT_API_KEY:-}}"
echo "LIVEKIT_API_SECRET=${{LIVEKIT_API_SECRET:-}}"
echo "DEV_LIVEKIT_CONTAINER=${{DEV_LIVEKIT_CONTAINER:-}}"
{after}
"#,
            script = repo_root().join(SCRIPT).display(),
        );
        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(body).current_dir(repo_root()).env_clear();
        cmd.env("PATH", format!("{}:/usr/bin:/bin", self.dir.path().display()))
            .env("HOME", self.dir.path())
            .env("STUB_LOG", self.log_path());
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("spawn bash");
        Outcome {
            succeeded: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            calls: read_calls(&self.log_path()),
        }
    }
}

fn read_calls(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .map(|s| s.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

impl Outcome {
    fn exported(&self, name: &str) -> String {
        let prefix = format!("{name}=");
        self.stdout
            .lines()
            .find_map(|l| l.strip_prefix(&prefix))
            .unwrap_or_else(|| panic!("{name} not printed; stdout:\n{}\nstderr:\n{}", self.stdout, self.stderr))
            .to_owned()
    }

    fn docker_calls(&self, subcommand: &str) -> Vec<&String> {
        let prefix = format!("docker {subcommand} ");
        self.calls.iter().filter(|c| c.starts_with(&prefix)).collect()
    }

    /// The host ports a `docker run` call publishes, in order (signal, ICE/TCP, ICE/UDP).
    fn published_ports(run: &str) -> Vec<(u16, u16)> {
        let words: Vec<&str> = run.split_whitespace().collect();
        words
            .windows(2)
            .filter(|w| w[0] == "-p")
            .map(|w| {
                let spec = w[1].trim_end_matches("/udp");
                let (host, container) = spec.split_once(':').expect("host:container");
                (host.parse().expect("host port"), container.parse().expect("container port"))
            })
            .collect()
    }

    fn exported_port(&self) -> u16 {
        self.exported("LIVEKIT_URL")
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .expect("LIVEKIT_URL ends in a port")
    }

    fn assert_succeeded(&self) -> &Self {
        assert!(self.succeeded, "expected success; stderr:\n{}", self.stderr);
        self
    }

    fn assert_failed_saying(&self, needle: &str) -> &Self {
        assert!(!self.succeeded, "expected failure; stdout:\n{}", self.stdout);
        assert!(
            self.stderr.contains(needle),
            "stderr must say {needle:?}; got:\n{}",
            self.stderr
        );
        self
    }
}

fn pinned_image() -> String {
    fs::read_to_string(repo_root().join(".config/livekit-server.image"))
        .expect("read pinned image")
        .trim()
        .to_owned()
}

/// `bash -n` must accept the script: `./web-dev` sources it under `set -euo pipefail`, so a syntax
/// error there kills the launcher before the daemon starts.
#[test]
fn dev_livekit_script_bash_syntax_is_valid() {
    tddy_e2e::dev_script_contract::verify_syntax(&repo_root().join(SCRIPT));
}

/// No credentials in the environment → a container of the pinned image is started in `--dev` mode
/// on ephemeral ports, and the daemon is pointed at it with LiveKit's dev credentials.
#[test]
fn without_credentials_it_starts_its_own_container_and_points_the_daemon_at_it() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env();

    // When
    let outcome = launcher.run("");

    // Then
    outcome.assert_succeeded();
    let runs = outcome.docker_calls("run");
    assert_eq!(runs.len(), 1, "exactly one container started; calls: {:#?}", outcome.calls);
    assert!(runs[0].contains(&pinned_image()), "uses the pinned image: {}", runs[0]);
    assert!(runs[0].contains("--dev"), "runs in --dev mode: {}", runs[0]);
    let port = outcome.exported_port();
    assert!((49152..=65533).contains(&port), "signal port {port} is in the ephemeral range");
    assert_eq!(outcome.exported("LIVEKIT_URL"), format!("ws://127.0.0.1:{port}"));
    assert_eq!(outcome.exported("LIVEKIT_PUBLIC_URL"), format!("ws://127.0.0.1:{port}"));
    assert_eq!(outcome.exported("LIVEKIT_API_KEY"), "devkey");
    assert_eq!(outcome.exported("LIVEKIT_API_SECRET"), "secret");
    assert!(!outcome.exported("DEV_LIVEKIT_CONTAINER").is_empty());
}

/// LiveKit embeds its container ports in ICE candidates, so every published port must keep its
/// number on both sides — an ephemeral host port remapped onto a fixed container port would leave
/// media unreachable.
#[test]
fn every_published_port_keeps_its_number_inside_the_container() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env();

    // When
    let outcome = launcher.run("");

    // Then
    outcome.assert_succeeded();
    let runs = outcome.docker_calls("run");
    let ports = Outcome::published_ports(runs[0]);
    assert_eq!(ports.len(), 3, "signal, ICE/TCP and ICE/UDP published: {}", runs[0]);
    for (host, container) in &ports {
        assert_eq!(host, container, "published 1:1: {}", runs[0]);
    }
    assert_eq!(ports[0].0, outcome.exported_port(), "LIVEKIT_URL names the signal port");
}

/// The container is removed on exit — and only the one this run started.
#[test]
fn on_exit_it_removes_the_container_it_started() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env();

    // When
    let outcome = launcher.run("");

    // Then
    outcome.assert_succeeded();
    let container = outcome.exported("DEV_LIVEKIT_CONTAINER");
    let removals = outcome.docker_calls("rm");
    assert_eq!(removals.len(), 1, "one removal; calls: {:#?}", outcome.calls);
    assert!(
        removals[0].ends_with(&format!(" {container}")),
        "removes {container} by name: {}",
        removals[0]
    );
}

/// An error exit after the container started still removes it: the EXIT trap runs on `set -e`
/// failures and on `exit 1` alike.
#[test]
fn an_error_exit_after_start_still_removes_the_container() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env();

    // When
    let outcome = launcher.run("false");

    // Then
    assert!(!outcome.succeeded);
    let container = outcome.exported("DEV_LIVEKIT_CONTAINER");
    assert!(
        outcome.docker_calls("rm").iter().any(|c| c.ends_with(&format!(" {container}"))),
        "container {container} removed after the error; calls: {:#?}",
        outcome.calls
    );
}

/// A port block already held by something else (another LiveKit, a second `./web-dev`) is left
/// alone: a fresh ephemeral block is tried instead, and nothing pre-existing is stopped.
#[test]
fn a_taken_port_block_is_retried_on_another_without_touching_its_holder() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env().with_first_port_blocks_taken(2);

    // When
    let outcome = launcher.run("");

    // Then
    outcome.assert_succeeded();
    let runs = outcome.docker_calls("run");
    assert_eq!(runs.len(), 3, "two taken blocks, then one that starts: {:#?}", outcome.calls);
    assert_eq!(
        Outcome::published_ports(runs[2])[0].0,
        outcome.exported_port(),
        "LIVEKIT_URL names the block that started"
    );
    let container = outcome.exported("DEV_LIVEKIT_CONTAINER");
    for call in outcome.calls.iter().filter(|c| c.starts_with("docker rm") || c.starts_with("docker stop") || c.starts_with("docker kill")) {
        assert!(
            call.contains("tddy-web-dev-livekit-"),
            "only this run's own containers may be removed, got: {call}"
        );
    }
    assert!(outcome.docker_calls("rm").iter().any(|c| c.ends_with(&format!(" {container}"))));
}

/// Complete credentials in `.env` → that server is reused: no container, nothing exported over it.
#[test]
fn complete_credentials_reuse_the_existing_server_and_start_no_container() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env().with_external_livekit("wss://lk.example.test");

    // When
    let outcome = launcher.run("");

    // Then
    outcome.assert_succeeded();
    assert!(outcome.docker_calls("run").is_empty(), "no container: {:#?}", outcome.calls);
    assert!(outcome.docker_calls("rm").is_empty(), "nothing removed: {:#?}", outcome.calls);
    assert_eq!(outcome.exported("LIVEKIT_URL"), "wss://lk.example.test");
    assert_eq!(outcome.exported("LIVEKIT_API_KEY"), "external-key");
    assert_eq!(outcome.exported("DEV_LIVEKIT_CONTAINER"), "");
}

/// A reused server that does not answer is a startup error naming it — not a daemon that retries a
/// dead URL forever while the web UI shows nothing.
#[test]
fn an_unreachable_reused_server_is_a_startup_error_naming_the_url() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env()
        .with_external_livekit("ws://lk.example.test:7880")
        .with_livekit_never_answering();

    // When
    let outcome = launcher.run("");

    // Then
    outcome.assert_failed_saying("ws://lk.example.test:7880");
    assert!(outcome.docker_calls("run").is_empty(), "no fallback container: {:#?}", outcome.calls);
}

/// Some LiveKit variables but not all three is ambiguous — the daemon would mix `.env` values with
/// a container's — so it is an error naming what is missing.
#[test]
fn partial_credentials_are_an_error_naming_the_missing_variables() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env().with("LIVEKIT_URL", "ws://lk.example.test");

    // When
    let outcome = launcher.run("");

    // Then
    outcome
        .assert_failed_saying("LIVEKIT_API_KEY")
        .assert_failed_saying("LIVEKIT_API_SECRET");
    assert!(outcome.docker_calls("run").is_empty());
}

/// Docker is the primary path, so Docker not running is a startup error that says so.
#[test]
fn docker_not_running_is_a_startup_error() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env().with_docker_not_running();

    // When
    let outcome = launcher.run("");

    // Then
    outcome.assert_failed_saying("Docker");
    assert!(outcome.docker_calls("run").is_empty());
}

/// A container that never answers is an error carrying its log tail, and it is still removed.
#[test]
fn a_container_that_never_becomes_ready_is_an_error_and_is_removed() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env().with_livekit_never_answering();

    // When
    let outcome = launcher.run("");

    // Then
    outcome.assert_failed_saying("stub log line");
    assert_eq!(outcome.docker_calls("run").len(), 1);
    assert!(
        outcome.docker_calls("rm").iter().any(|c| c.contains("tddy-web-dev-livekit-")),
        "unready container removed; calls: {:#?}",
        outcome.calls
    );
}

/// `./web-dev` must source the script, register `dev_livekit_down` on EXIT, and bring LiveKit up
/// after the build (a failed build starts nothing) and before the daemon starts.
#[test]
fn web_dev_brings_livekit_up_between_build_and_daemon_and_down_on_exit() {
    // Given
    let web_dev = fs::read_to_string(repo_root().join("web-dev")).expect("read web-dev");

    // When
    let pos = |needle: &str| {
        web_dev
            .find(needle)
            .unwrap_or_else(|| panic!("web-dev must contain {needle:?}"))
    };

    // Then
    assert!(web_dev.contains(&format!("source {SCRIPT}")), "web-dev sources {SCRIPT}");
    let trap_line = web_dev
        .lines()
        .find(|l| l.trim_start().starts_with("trap ") && l.trim_end().ends_with(" EXIT"))
        .expect("web-dev registers an EXIT trap");
    let handler = trap_line
        .trim()
        .strip_prefix("trap ")
        .and_then(|rest| rest.strip_suffix(" EXIT"))
        .expect("trap <handler> EXIT")
        .trim_matches(|c| c == '\'' || c == '"');
    let calls_down = handler.contains("dev_livekit_down")
        || web_dev
            .split(&format!("{handler}() {{"))
            .nth(1)
            .and_then(|body| body.split("\n}").next())
            .is_some_and(|body| body.contains("dev_livekit_down"));
    assert!(calls_down, "the EXIT trap handler {handler:?} must call dev_livekit_down");
    let up = pos("\ndev_livekit_up\n");
    assert!(pos("./dev cargo build") < up, "LiveKit starts after the build");
    assert!(up < pos("./dev \"$DAEMON_BIN\""), "LiveKit starts before the daemon");
}

/// Every attempted block taken is a startup error, not an endless loop.
#[test]
fn every_attempted_block_taken_is_a_startup_error() {
    // Given
    let launcher = Launcher::with_no_livekit_in_env().with_first_port_blocks_taken(1000);

    // When
    let outcome = launcher.run("");

    // Then
    outcome.assert_failed_saying("no free LiveKit port block");
}

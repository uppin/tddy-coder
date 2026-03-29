//! Connect / HTTP integration acceptance for remote sandboxes (PRD Testing Plan).
//!
//! Spawns `tddy-daemon` with a minimal web bundle and asserts RPC behaviour expected once
//! `remote_sandbox.v1.RemoteSandboxService` is registered.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use prost::Message;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue};
use scopeguard::guard;
use tddy_service::proto::remote_sandbox_v1::{
    ExecNonInteractiveRequest, ExecNonInteractiveResponse, PutObjectRequest, StatObjectRequest,
};

const REMOTE_SANDBOX_SERVICE: &str = "remote_sandbox.v1.RemoteSandboxService";
const METHOD_EXEC: &str = "ExecNonInteractive";
const METHOD_PUT: &str = "PutObject";
const METHOD_STAT: &str = "StatObject";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("manifest under packages/tddy-daemon")
        .to_path_buf()
}

fn daemon_bin() -> PathBuf {
    let rel = std::env::var("CARGO_BIN_EXE_tddy-daemon")
        .unwrap_or_else(|_| "target/debug/tddy-daemon".to_string());
    let p = workspace_root().join(&rel);
    if p.exists() {
        p
    } else {
        PathBuf::from(rel)
    }
}

fn pick_free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    l.local_addr().expect("addr").port()
}

fn write_minimal_bundle(dir: &Path) {
    std::fs::write(dir.join("index.html"), "<!doctype html><title>tddy</title>")
        .expect("index.html");
}

fn write_daemon_config(path: &Path, port: u16, bundle: &Path) {
    let yaml = format!(
        r#"
listen:
  web_port: {port}
  web_host: "127.0.0.1"
web_bundle_path: {bundle}
github:
  stub: true
  client_id: "acceptance-client"
users:
  - github_user: "acceptance-user"
    os_user: "acceptance-os"
"#,
        port = port,
        bundle = bundle.display()
    );
    std::fs::write(path, yaml).expect("daemon.yaml");
}

struct DaemonProcess {
    child: Child,
    base_url: String,
}

fn spawn_daemon() -> DaemonProcess {
    let bin = daemon_bin();
    assert!(
        bin.exists(),
        "tddy-daemon binary missing at {} (cargo build -p tddy-daemon)",
        bin.display()
    );

    let tmp = tempfile::tempdir().expect("tempdir");
    let bundle = tmp.path().join("bundle");
    std::fs::create_dir_all(&bundle).expect("bundle dir");
    write_minimal_bundle(&bundle);

    let port = pick_free_port();
    let cfg_path = tmp.path().join("daemon.yaml");
    write_daemon_config(&cfg_path, port, &bundle);

    let mut child = Command::new(&bin)
        .arg("-c")
        .arg(&cfg_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tddy-daemon");

    let base_url = format!("http://127.0.0.1:{port}");
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("client");

    for _ in 0..40 {
        if let Ok(resp) = client.get(format!("{base_url}/api/config")).send() {
            if resp.status().is_success() {
                return DaemonProcess { child, base_url };
            }
        }
        thread::sleep(Duration::from_millis(150));
    }

    let _ = child.kill();
    panic!("tddy-daemon did not become ready at {base_url}");
}

fn encode_put(session: &str, path: &str, content: &[u8]) -> Vec<u8> {
    PutObjectRequest {
        session: session.to_string(),
        path: path.to_string(),
        content: content.to_vec(),
    }
    .encode_to_vec()
}

fn encode_stat(session: &str, path: &str) -> Vec<u8> {
    StatObjectRequest {
        session: session.to_string(),
        path: path.to_string(),
    }
    .encode_to_vec()
}

fn encode_exec(session: &str, argv: &[String]) -> Vec<u8> {
    ExecNonInteractiveRequest {
        argv_json: serde_json::to_string(argv).expect("argv json"),
        session: session.to_string(),
    }
    .encode_to_vec()
}

fn connect_post(base: &str, method: &str, body: &[u8]) -> reqwest::blocking::Response {
    let url = format!(
        "{}/rpc/{}/{}",
        base.trim_end_matches('/'),
        REMOTE_SANDBOX_SERVICE,
        method
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        HeaderValue::from_static("application/proto"),
    );
    headers.insert("Connect-Protocol-Version", HeaderValue::from_static("1"));
    Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client")
        .post(url)
        .headers(headers)
        .body(body.to_vec())
        .send()
        .expect("HTTP POST to Connect path")
}

fn connect_post_timeout(
    base: &str,
    method: &str,
    body: &[u8],
    timeout_secs: u64,
) -> reqwest::blocking::Response {
    let url = format!(
        "{}/rpc/{}/{}",
        base.trim_end_matches('/'),
        REMOTE_SANDBOX_SERVICE,
        method
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        HeaderValue::from_static("application/proto"),
    );
    headers.insert("Connect-Protocol-Version", HeaderValue::from_static("1"));
    Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .expect("client")
        .post(url)
        .headers(headers)
        .body(body.to_vec())
        .send()
        .expect("HTTP POST to Connect path")
}

#[test]
fn shell_remote_exit_code_propagation_via_connect() {
    let d = spawn_daemon();
    let _guard = guard(d.child, |mut c| {
        let _ = c.kill();
        let _ = c.wait();
    });

    let resp = connect_post(&d.base_url, METHOD_EXEC, &[]);
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::OK,
        "ExecNonInteractive must return HTTP 200 with exit status in body once implemented; got {}",
        resp.status()
    );
}

#[test]
fn vfs_rsync_push_checksum() {
    assert!(
        Command::new("rsync")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "rsync must be installed for this acceptance test (PRD E2E)"
    );

    let d = spawn_daemon();
    let _guard = guard(d.child, |mut c| {
        let _ = c.kill();
        let _ = c.wait();
    });

    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    let payload = b"acceptance-rsync-push-payload\n";
    std::fs::write(src.join("payload.bin"), payload).expect("payload");

    let tddy_remote = std::env::var("CARGO_BIN_EXE_tddy-remote")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root().join("target/debug/tddy-remote"));
    assert!(
        tddy_remote.exists(),
        "tddy-remote binary missing at {}",
        tddy_remote.display()
    );

    let remote_cfg = tmp.path().join("remote.yaml");
    std::fs::write(
        &remote_cfg,
        format!(
            r#"default_authority: "local"
authorities:
  - id: "local"
    connect_base_url: "{}"
"#,
            d.base_url
        ),
    )
    .expect("remote config");

    let dest = tmp.path().join("dest");
    std::fs::create_dir_all(&dest).expect("dest");

    const RSYNC_SESSION: &str = "acceptance-rsync-push";
    std::env::set_var("TDDY_REMOTE_RSYNC_SESSION", RSYNC_SESSION);

    let status = Command::new("rsync")
        .env(
            "RSYNC_RSH",
            format!(
                "{} --config {}",
                tddy_remote.display(),
                remote_cfg.display()
            ),
        )
        .args([
            "-a",
            "--checksum",
            &format!("{}/", src.display()),
            &format!(
                "local:acceptance-rsync-push/{}/",
                dest.display().to_string().trim_start_matches('/')
            ),
        ])
        .status()
        .expect("rsync");

    std::env::remove_var("TDDY_REMOTE_RSYNC_SESSION");

    assert!(
        status.success(),
        "rsync push must exit 0 (PRD vfs_rsync_push_checksum)"
    );

    let rel_path = format!(
        "acceptance-rsync-push/{}/payload.bin",
        dest.display().to_string().trim_start_matches('/')
    );
    let argv = vec!["sha256sum".to_string(), rel_path];
    let body = encode_exec(RSYNC_SESSION, &argv);
    let resp = connect_post_timeout(&d.base_url, METHOD_EXEC, &body, 30);
    assert!(
        resp.status().is_success(),
        "ExecNonInteractive sha256sum must return HTTP 2xx; got {}",
        resp.status()
    );
    let exec_bytes = resp.bytes().expect("exec body");
    let exec_out: ExecNonInteractiveResponse =
        Message::decode(exec_bytes.as_ref()).expect("decode ExecNonInteractiveResponse");
    assert_eq!(
        exec_out.exit_code,
        0,
        "sha256sum must exit 0 in sandbox (stdout_len={})",
        exec_out.stdout.len()
    );
    let line = String::from_utf8_lossy(&exec_out.stdout);
    let digest = line
        .split_whitespace()
        .next()
        .expect("sha256sum stdout line");
    assert_eq!(
        digest,
        sha256_hex(payload),
        "SHA-256 after push must match source"
    );
}

#[test]
fn vfs_rsync_pull_checksum() {
    assert!(
        Command::new("rsync")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "rsync must be installed for this acceptance test (PRD E2E)"
    );

    let d = spawn_daemon();
    let _guard = guard(d.child, |mut c| {
        let _ = c.kill();
        let _ = c.wait();
    });

    let tmp = tempfile::tempdir().expect("tempdir");
    let local = tmp.path().join("local_pull");
    std::fs::create_dir_all(&local).expect("local");

    const RSYNC_PULL_SESSION: &str = "acceptance-rsync-pull-session";
    let expected = b"acceptance-rsync-pull-expected-bytes";
    let seed_put = connect_post(
        &d.base_url,
        METHOD_PUT,
        &encode_put(
            RSYNC_PULL_SESSION,
            "acceptance-rsync-pull/payload.bin",
            expected,
        ),
    );
    assert!(
        seed_put.status().is_success(),
        "seed PutObject for pull test must succeed; got {}",
        seed_put.status()
    );

    let tddy_remote = std::env::var("CARGO_BIN_EXE_tddy-remote")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root().join("target/debug/tddy-remote"));

    let remote_cfg = tmp.path().join("remote.yaml");
    std::fs::write(
        &remote_cfg,
        format!(
            r#"default_authority: "local"
authorities:
  - id: "local"
    connect_base_url: "{}"
"#,
            d.base_url
        ),
    )
    .expect("remote config");

    let status = Command::new("rsync")
        .env("TDDY_REMOTE_RSYNC_SESSION", RSYNC_PULL_SESSION)
        .env(
            "RSYNC_RSH",
            format!(
                "{} --config {}",
                tddy_remote.display(),
                remote_cfg.display()
            ),
        )
        .args([
            "-a",
            "--checksum",
            "local:acceptance-rsync-pull/payload.bin",
            &format!("{}/", local.display()),
        ])
        .status()
        .expect("rsync");

    assert!(
        status.success(),
        "rsync pull must exit 0 (PRD vfs_rsync_pull_checksum)"
    );

    let got = std::fs::read(local.join("payload.bin")).expect("pulled file");
    assert_eq!(
        sha256_hex(&got),
        sha256_hex(expected),
        "SHA-256 after pull must match sandbox file"
    );
}

#[test]
fn concurrent_sandboxes_isolated() {
    let d = spawn_daemon();
    let _guard = guard(d.child, |mut c| {
        let _ = c.kill();
        let _ = c.wait();
    });

    let put_a = connect_post(
        &d.base_url,
        METHOD_PUT,
        &encode_put("a", "/only-a.txt", &[]),
    );
    assert!(
        put_a.status().is_success(),
        "PutObject for session A must succeed (HTTP 2xx); got {}",
        put_a.status()
    );

    let stat_b = connect_post(&d.base_url, METHOD_STAT, &encode_stat("b", "/only-a.txt"));
    assert_eq!(
        stat_b.status(),
        reqwest::StatusCode::NOT_FOUND,
        "session B must not observe files created in session A's default tree (expect 404 / not found from StatObject)"
    );
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

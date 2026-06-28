//! Acceptance: sandbox → host egress routing over TLS, exercised without a real jail.
//!
//! Proves the full path a sandboxed agent's outbound HTTPS takes:
//!
//! ```text
//! test app (curl) → in-jail HTTPS_PROXY shim → SessionChannel (AF_UNIX) → host relay → TLS server
//! ```
//!
//! The runner is driven in-process via `run_sandbox_runner` (no Seatbelt / no cgroups), the host
//! side uses the shared, reusable `run_host_relay`, and the upstream is a real TLS server with a
//! self-signed certificate. The host relay only moves bytes — the TLS handshake is end-to-end
//! between curl and the server — so a `TLS_ECHO` body proving the round-trip can only appear if the
//! tunnel routed the encrypted bytes faithfully in both directions.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use tddy_sandbox_runner::{
    connect_sandbox_client_uds, run_host_relay, run_sandbox_runner, ExecuteToolResponse,
    HostRelayConfig, HostToolHandler, SandboxRunnerArgs,
};
use tddy_testing_commons::CONNECT_PROBE_TUNNEL_OK;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

const SESSION_ID: &str = "sandbox-egress-relay-tls-session";
const TEST_MODEL: &str = "claude-opus-4-8";
const TLS_ECHO: &str = "TLS_ECHO";
const TEST_TIMEOUT: Duration = Duration::from_secs(45);

/// A host tool handler that performs no real work — egress routing is what this test exercises.
struct StubToolHandler;

#[async_trait::async_trait]
impl HostToolHandler for StubToolHandler {
    async fn execute(
        &self,
        _session_id: &str,
        tool_name: &str,
        _args_json: &str,
    ) -> ExecuteToolResponse {
        ExecuteToolResponse {
            result_json: format!(r#"{{"tool":"{tool_name}"}}"#),
            is_error: false,
            ..Default::default()
        }
    }
}

fn tddy_tools_path() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tddy-tools"))
}

/// Start a loopback TLS server with a self-signed cert for `127.0.0.1`. Returns its port and the
/// certificate PEM (so curl can trust it via `--cacert`). Each accepted connection completes the
/// TLS handshake and replies with a fixed HTTP body containing [`TLS_ECHO`].
async fn spawn_tls_echo_server() -> (u16, String) {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let certified = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_string()])
        .expect("generate self-signed cert");
    let cert_pem = certified.cert.pem();
    let cert_der = certified.cert.der().clone();
    let key_der = certified.key_pair.serialize_der();

    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
        key_der,
    ));
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key)
        .expect("build rustls server config");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind tls server");
    let port = listener.local_addr().expect("local addr").port();

    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                continue;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = [0u8; 1024];
                let _ = tls.read(&mut buf).await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}\n",
                    TLS_ECHO.len() + 1,
                    TLS_ECHO
                );
                let _ = tls.write_all(response.as_bytes()).await;
                let _ = tls.shutdown().await;
            });
        }
    });

    (port, cert_pem)
}

/// Fake claude that drives an HTTPS `CONNECT` tunnel through `$HTTPS_PROXY` to the TLS server.
/// Prints [`CONNECT_PROBE_TUNNEL_OK`] on a `TLS_ECHO` body. Uses `-k`: the TLS handshake still runs
/// end-to-end through the tunnel (this test proves byte routing, not certificate trust — the test
/// cert's SAN is a hostname, not the `127.0.0.1` literal curl dials).
fn write_tls_connect_proxy_claude_script(dir: &Path) -> PathBuf {
    let script = dir.join("tls_connect_proxy_claude.sh");
    let body = format!(
        r#"#!/bin/sh
echo "ARGV: $@"
HOST="${{TDDY_EGRESS_PROBE_HOST:-127.0.0.1}}"
PORT="${{TDDY_EGRESS_PROBE_PORT:-9}}"
PROXY="${{HTTPS_PROXY:-${{TDDY_EGRESS_SHIM:-}}}}"

if [ -z "$PROXY" ]; then
  echo "CONNECT_PROBE: tunnel=unset"
elif curl -s --proxytunnel -x "$PROXY" -k --connect-timeout 5 --max-time 10 "https://${{HOST}}:${{PORT}}/llm" 2>/dev/null | grep -q {echo}; then
  echo "{ok}"
else
  echo "CONNECT_PROBE: tunnel=denied"
fi

exec cat
"#,
        echo = TLS_ECHO,
        ok = CONNECT_PROBE_TUNNEL_OK,
    );
    std::fs::write(&script, body).expect("write tls connect proxy script");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
}

async fn wait_for_ready(ready_marker: &Path, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if ready_marker.exists() {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for sandbox-runner ready marker at {}",
                ready_marker.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// **routes_https_egress_from_jail_through_the_host_relay_to_a_tls_server**: with the runner serving
/// its `SessionChannel` over AF_UNIX and the host attached via the shared `run_host_relay`, a
/// CONNECT tunnel from the in-jail agent reaches a real TLS server and the encrypted round-trip
/// succeeds (`CONNECT_PROBE: tunnel=ok`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn routes_https_egress_from_jail_through_the_host_relay_to_a_tls_server() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        // Given — a real TLS server and a jail context configured for AF_UNIX control channel.
        let (tls_port, _cert_pem) = spawn_tls_echo_server().await;
        let tmp = tempfile::tempdir().unwrap();
        let context_dir = tmp.path().join("context");
        let egress = tmp.path().join("egress");
        std::fs::create_dir_all(&context_dir).unwrap();
        std::fs::create_dir_all(&egress).unwrap();
        std::env::set_var("TDDY_SANDBOX_EGRESS_DIR", &egress);
        std::env::set_var("TDDY_SANDBOX_SESSION_ID", SESSION_ID);
        std::env::set_var("TDDY_EGRESS_PROBE_HOST", "127.0.0.1");
        std::env::set_var("TDDY_EGRESS_PROBE_PORT", tls_port.to_string());

        let claude = write_tls_connect_proxy_claude_script(tmp.path());
        let uds_path = tmp.path().join("sandbox.grpc.sock");
        let ready_marker = tmp.path().join("sandbox.ready");
        let args = SandboxRunnerArgs {
            session_id: SESSION_ID.to_string(),
            context_dir,
            grpc_socket: uds_path.clone(),
            tool_ipc_socket: tmp.path().join("tool_ipc.sock"),
            tddy_tools_path: tddy_tools_path(),
            claude_binary: claude.to_string_lossy().to_string(),
            model: TEST_MODEL.to_string(),
            ready_marker: ready_marker.clone(),
            permission_mode: "auto".to_string(),
            grpc_listen_port: None,
            egress_shim_port: None,
            grpc_uds: Some(uds_path.clone()),
        };

        let runner_task = tokio::spawn(async move {
            let _ = run_sandbox_runner(args).await;
        });
        wait_for_ready(&ready_marker, Duration::from_secs(15)).await;

        // When — the host attaches via the reusable relay and the agent issues its CONNECT.
        let client = connect_sandbox_client_uds(&uds_path)
            .await
            .expect("dial sandbox grpc over AF_UNIX");
        let (terminal_tx, mut terminal_rx) = mpsc::unbounded_channel::<Bytes>();
        let (_stdin_tx, stdin_rx) = mpsc::unbounded_channel::<Bytes>();
        let terminal = Arc::new(Mutex::new(String::new()));
        let terminal_collector = Arc::clone(&terminal);
        tokio::spawn(async move {
            while let Some(chunk) = terminal_rx.recv().await {
                terminal_collector
                    .lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&chunk));
            }
        });

        let _relay = run_host_relay(
            client,
            StubToolHandler,
            HostRelayConfig::new(SESSION_ID, terminal_tx),
            stdin_rx,
        )
        .await
        .expect("start host relay");

        // Then — the encrypted round-trip succeeds through the tunnel.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let terminal_text = loop {
            let text = terminal.lock().unwrap().clone();
            if text.contains(CONNECT_PROBE_TUNNEL_OK) || tokio::time::Instant::now() >= deadline {
                break text;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        assert!(
            terminal_text.contains(CONNECT_PROBE_TUNNEL_OK),
            "TLS egress must route jail → host relay → TLS server, got:\n{terminal_text}"
        );

        runner_task.abort();
    })
    .await
    .expect("routes_https_egress_from_jail_through_the_host_relay_to_a_tls_server timed out");
}

//! Acceptance: sandbox-runner behavior with `tddy-demo-tui` and SessionChannel LLM egress.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_sandbox::format_egress_logs;
use tddy_sandbox_darwin::{run_sandbox_runner, SandboxRunnerArgs};
use tddy_testing_commons::{
    write_connect_proxy_claude_script, write_egress_probe_claude_script, SandboxSessionChannelHost,
    CONNECT_PROBE_TUNNEL_OK, EGRESS_PROBE_SESSION_CHANNEL_OK,
};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

const SESSION_ID: &str = "sandbox-runner-behavior-session";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_TIMEOUT: Duration = Duration::from_secs(45);

/// Serializes tests that mutate the process-global `TDDY_EGRESS_PROBE_*` env so they don't clobber
/// each other under cargo's default parallel test execution.
fn egress_env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn tddy_tools_path() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tddy-tools"))
}

fn demo_tui_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-demo-tui")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tddy-demo-tui")
        })
}

async fn wait_for_ready(ready_marker: &Path, egress: &Path, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if ready_marker.exists() {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for sandbox-runner ready marker at {}\n{}",
                ready_marker.display(),
                format_egress_logs(egress)
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn runner_args(tmp: &Path, claude_binary: &Path) -> (SandboxRunnerArgs, PathBuf) {
    let context_dir = tmp.join("context");
    let egress = tmp.join("egress");
    std::fs::create_dir_all(&context_dir).unwrap();
    std::fs::create_dir_all(&egress).unwrap();
    std::env::set_var("TDDY_SANDBOX_EGRESS_DIR", &egress);
    std::env::set_var("TDDY_SANDBOX_SESSION_ID", SESSION_ID);

    let args = SandboxRunnerArgs {
        session_id: SESSION_ID.to_string(),
        context_dir,
        cwd: None,
        claude_binary: claude_binary.to_string_lossy().to_string(),
        model: TEST_MODEL.to_string(),
        grpc_socket: Some(tmp.join("sandbox.grpc.sock")),
        tool_ipc_socket: tmp.join("tool_ipc.sock"),
        tddy_tools_path: Some(tddy_tools_path()),
        ready_marker: tmp.join("sandbox.ready"),
        permission_mode: "auto".to_string(),
        grpc_listen_port: None,
        egress_shim_port: None,
        grpc_uds: None,
        pty_command: vec![],
        stdio: false,
        initial_cols: 80,
        initial_rows: 24,
    };
    (args, egress)
}

async fn stop_runner(runner_task: tokio::task::JoinHandle<()>) {
    runner_task.abort();
    let _ = tokio::time::timeout(Duration::from_secs(2), runner_task).await;
}

async fn spawn_llm_echo_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind echo server");
    let port = listener.local_addr().expect("local addr").port();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                let response =
                    b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\nLLM_ECHO\n";
                let _ = stream.write_all(response).await;
            });
        }
    });
    port
}

/// **sandbox_runner_streams_demo_tui_dimensions_on_session_channel**: the Cypress e2e fake claude
/// (`tddy-demo-tui`) draws `DEMO TUI W=` on the PTY bridged through SessionChannel.
#[tokio::test]
async fn sandbox_runner_streams_demo_tui_dimensions_on_session_channel() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        // Given
        let demo_tui = demo_tui_binary();
        assert!(
            demo_tui.exists(),
            "build tddy-demo-tui before running: cargo build -p tddy-demo-tui"
        );
        let tmp = tempfile::tempdir().unwrap();
        let (args, egress) = runner_args(tmp.path(), &demo_tui);
        let ready_marker = args.ready_marker.clone();

        let runner_task = tokio::spawn(async move {
            let _ = run_sandbox_runner(args).await;
        });
        wait_for_ready(&ready_marker, &egress, Duration::from_secs(15)).await;
        let host = SandboxSessionChannelHost::connect(&ready_marker, SESSION_ID).await;

        // When
        let terminal_text = host
            .collect_terminal_until(Duration::from_secs(10), "DEMO TUI W=")
            .await;

        // Then
        assert!(
            terminal_text.contains("DEMO TUI W="),
            "demo-tui must render PTY dimensions, got:\n{terminal_text}\n{}",
            format_egress_logs(&egress)
        );

        stop_runner(runner_task).await;
    })
    .await
    .expect("sandbox_runner_streams_demo_tui_dimensions_on_session_channel timed out");
}

/// **sandbox_runner_relays_claude_llm_egress_via_session_channel**: the in-jail HTTP shim sends
/// `EgressRequest` on SessionChannel; the host performs outbound HTTP and the probe script prints
/// `EGRESS_PROBE: session_channel=ok`.
#[tokio::test]
async fn sandbox_runner_relays_claude_llm_egress_via_session_channel() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let _env_guard = egress_env_lock().lock().await;
        // Given
        let echo_port = spawn_llm_echo_server().await;
        std::env::set_var("TDDY_EGRESS_PROBE_HOST", "127.0.0.1");
        std::env::set_var("TDDY_EGRESS_PROBE_PORT", echo_port.to_string());
        std::env::set_var(
            "TDDY_EGRESS_PROBE_URL",
            format!("http://127.0.0.1:{echo_port}/llm"),
        );

        let tmp = tempfile::tempdir().unwrap();
        let probe_claude = write_egress_probe_claude_script(tmp.path());
        let (args, egress) = runner_args(tmp.path(), &probe_claude);
        let ready_marker = args.ready_marker.clone();

        let runner_task = tokio::spawn(async move {
            let _ = run_sandbox_runner(args).await;
        });
        wait_for_ready(&ready_marker, &egress, Duration::from_secs(15)).await;
        let host = SandboxSessionChannelHost::connect(&ready_marker, SESSION_ID).await;

        // When
        let terminal_text = host
            .collect_terminal_until(Duration::from_secs(10), EGRESS_PROBE_SESSION_CHANNEL_OK)
            .await;

        // Then
        assert!(
            terminal_text.contains(EGRESS_PROBE_SESSION_CHANNEL_OK),
            "LLM egress must relay via SessionChannel EgressRequest, got:\n{terminal_text}\n{}",
            format_egress_logs(&egress)
        );

        stop_runner(runner_task).await;
    })
    .await
    .expect("sandbox_runner_relays_claude_llm_egress_via_session_channel timed out");
}

/// **sandbox_runner_tunnels_https_proxy_connect_via_session_channel**: with `HTTPS_PROXY` set to
/// the in-jail egress shim, a `CONNECT` from the agent is relayed over SessionChannel as a raw TCP
/// tunnel; the host dials the real socket and bytes round-trip (`CONNECT_PROBE: tunnel=ok`).
#[tokio::test]
async fn sandbox_runner_tunnels_https_proxy_connect_via_session_channel() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let _env_guard = egress_env_lock().lock().await;
        // Given
        let echo_port = spawn_llm_echo_server().await;
        std::env::set_var("TDDY_EGRESS_PROBE_HOST", "127.0.0.1");
        std::env::set_var("TDDY_EGRESS_PROBE_PORT", echo_port.to_string());

        let tmp = tempfile::tempdir().unwrap();
        let connect_claude = write_connect_proxy_claude_script(tmp.path());
        let (args, egress) = runner_args(tmp.path(), &connect_claude);
        let ready_marker = args.ready_marker.clone();

        let runner_task = tokio::spawn(async move {
            let _ = run_sandbox_runner(args).await;
        });
        wait_for_ready(&ready_marker, &egress, Duration::from_secs(15)).await;
        let host = SandboxSessionChannelHost::connect(&ready_marker, SESSION_ID).await;

        // When
        let terminal_text = host
            .collect_terminal_until(Duration::from_secs(10), CONNECT_PROBE_TUNNEL_OK)
            .await;

        // Then
        assert!(
            terminal_text.contains(CONNECT_PROBE_TUNNEL_OK),
            "HTTPS_PROXY CONNECT must tunnel via SessionChannel, got:\n{terminal_text}\n{}",
            format_egress_logs(&egress)
        );

        stop_runner(runner_task).await;
    })
    .await
    .expect("sandbox_runner_tunnels_https_proxy_connect_via_session_channel timed out");
}

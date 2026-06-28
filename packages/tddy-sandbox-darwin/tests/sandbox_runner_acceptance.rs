//! Acceptance: `tddy-sandbox-runner` gRPC server — SessionChannel, PTY, tool exec.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use tddy_sandbox::format_egress_logs;
use tddy_sandbox_darwin::{connect_sandbox_client, run_sandbox_runner, SandboxRunnerArgs};
use tddy_service::proto::connection::ExecuteToolResponse;
use tddy_service::tonic_sandbox::session_frame::Payload as SessionPayload;
use tddy_service::tonic_sandbox::{
    EchoRequest, EchoStreamFrame, HostPoll, SandboxInput, SessionFrame, SubscribeTerminal,
};
use tokio_stream::wrappers::ReceiverStream;

fn tddy_tools_path() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_tddy-tools")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tddy-tools"))
}

const SESSION_ID: &str = "sandbox-runner-test-session";
const TEST_MODEL: &str = "claude-opus-4-8";
const TEST_TIMEOUT: Duration = Duration::from_secs(45);

fn write_echo_argv_script(dir: &Path) -> PathBuf {
    let script = dir.join("stub_claude.sh");
    std::fs::write(&script, "#!/bin/sh\necho \"ARGV: $@\"\ncat\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
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

fn runner_args(tmp: &Path, stub_claude: &Path) -> (SandboxRunnerArgs, PathBuf) {
    let context_dir = tmp.join("context");
    let egress = tmp.join("egress");
    std::fs::create_dir_all(&context_dir).unwrap();
    std::fs::create_dir_all(&egress).unwrap();
    std::env::set_var("TDDY_SANDBOX_EGRESS_DIR", &egress);
    std::env::set_var("TDDY_SANDBOX_SESSION_ID", SESSION_ID);

    let args = SandboxRunnerArgs {
        session_id: SESSION_ID.to_string(),
        context_dir,
        claude_binary: stub_claude.to_string_lossy().to_string(),
        model: TEST_MODEL.to_string(),
        grpc_socket: tmp.join("sandbox.grpc.sock"),
        tool_ipc_socket: tmp.join("tool_ipc.sock"),
        tddy_tools_path: tddy_tools_path(),
        ready_marker: tmp.join("sandbox.ready"),
        permission_mode: "auto".to_string(),
        grpc_listen_port: None,
        egress_shim_port: None,
    };
    (args, egress)
}

async fn stop_runner(runner_task: tokio::task::JoinHandle<()>) {
    runner_task.abort();
    let _ = tokio::time::timeout(Duration::from_secs(2), runner_task).await;
}

/// Host-side SessionChannel loop (mirrors `dial_and_bridge` in the daemon).
async fn open_host_session_channel(ready_marker: &Path) -> tokio::task::JoinHandle<()> {
    let mut client = connect_sandbox_client(ready_marker)
        .await
        .expect("connect sandbox grpc");

    let (host_tx, host_rx) = tokio::sync::mpsc::channel(64);
    let host_stream = ReceiverStream::new(host_rx);
    let mut session = client
        .session_channel(host_stream)
        .await
        .expect("SessionChannel must open")
        .into_inner();

    host_tx
        .send(SessionFrame {
            payload: Some(SessionPayload::SubscribeTerminal(SubscribeTerminal {
                session_id: SESSION_ID.to_string(),
                terminal_id: "main".to_string(),
                initial_cols: 80,
                initial_rows: 24,
            })),
        })
        .await
        .expect("subscribe frame");

    let host_tx_poll = host_tx.clone();
    let reader = tokio::spawn(async move {
        while let Some(Ok(frame)) = session.next().await {
            if let Some(SessionPayload::ToolRequest(req)) = frame.payload {
                let resp = ExecuteToolResponse {
                    result_json: format!(r#"{{"path":"{}"}}"#, req.tool_name),
                    is_error: false,
                    ..Default::default()
                };
                let _ = host_tx
                    .send(SessionFrame {
                        payload: Some(SessionPayload::ToolResponse(resp)),
                    })
                    .await;
            }
        }
    });

    tokio::spawn(async move {
        let mut poll = tokio::time::interval(Duration::from_millis(25));
        loop {
            poll.tick().await;
            if host_tx_poll
                .send(SessionFrame {
                    payload: Some(SessionPayload::HostPoll(HostPoll {})),
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });

    reader
}

/// **sandbox_runner_echo_unary_round_trips**: a daemon client dials the runner and a unary
/// `Echo` RPC returns the same message — validates gRPC connectivity before bidi/tool-exec.
#[tokio::test]
async fn sandbox_runner_echo_unary_round_trips() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let stub_claude = write_echo_argv_script(tmp.path());
        let (args, egress) = runner_args(tmp.path(), &stub_claude);
        let ready_marker = args.ready_marker.clone();

        let runner_task = tokio::spawn(async move {
            let _ = run_sandbox_runner(args).await;
        });

        wait_for_ready(&ready_marker, &egress, Duration::from_secs(15)).await;

        let mut client = connect_sandbox_client(&ready_marker)
            .await
            .expect("connect sandbox grpc");

        // When
        let echo_message = "sandbox-grpc-ping";
        let resp = client
            .echo(EchoRequest {
                message: echo_message.to_string(),
            })
            .await
            .expect("Echo must succeed");

        // Then
        assert_eq!(
            resp.into_inner().message,
            echo_message,
            "Echo must round-trip the message\n{}",
            format_egress_logs(&egress)
        );

        stop_runner(runner_task).await;
    })
    .await
    .expect("sandbox_runner_echo_unary_round_trips timed out");
}

/// **sandbox_runner_echo_stream_bidi_round_trips**: a client opens the bidi `EchoStream` RPC,
/// sends one frame, and receives the same message echoed on the outbound stream.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sandbox_runner_echo_stream_bidi_round_trips() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let stub_claude = write_echo_argv_script(tmp.path());
        let (args, egress) = runner_args(tmp.path(), &stub_claude);
        let ready_marker = args.ready_marker.clone();

        let runner_task = tokio::spawn(async move {
            let _ = run_sandbox_runner(args).await;
        });

        wait_for_ready(&ready_marker, &egress, Duration::from_secs(15)).await;

        let mut client = connect_sandbox_client(&ready_marker)
            .await
            .expect("connect sandbox grpc");

        let echo_message = "sandbox-bidi-ping";
        let (request_tx, request_rx) = tokio::sync::mpsc::channel(4);
        let request_stream = ReceiverStream::new(request_rx);
        let mut echo_stream = client
            .echo_stream(request_stream)
            .await
            .expect("EchoStream must open")
            .into_inner();

        let egress_for_reader = egress.clone();
        let reader = tokio::spawn(async move {
            if let Some(Ok(frame)) = echo_stream.next().await {
                return frame.message;
            }
            panic!(
                "EchoStream closed before echo arrived\n{}",
                format_egress_logs(&egress_for_reader)
            );
        });

        // When
        request_tx
            .send(EchoStreamFrame {
                message: echo_message.to_string(),
            })
            .await
            .expect("send EchoStream frame");

        let echoed = tokio::time::timeout(Duration::from_secs(5), reader)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "EchoStream response timed out\n{}",
                    format_egress_logs(&egress)
                )
            })
            .expect("reader task");

        // Then
        assert_eq!(
            echoed,
            echo_message,
            "EchoStream must echo the inbound message\n{}",
            format_egress_logs(&egress)
        );

        stop_runner(runner_task).await;
    })
    .await
    .expect("sandbox_runner_echo_stream_bidi_round_trips timed out");
}

/// **sandbox_runner_session_channel_streams_pty_output_and_accepts_input**: host SessionChannel
/// receives PTY bytes from the stub claude and sends keystrokes via inbound terminal input.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sandbox_runner_session_channel_streams_pty_output_and_accepts_input() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let stub_claude = write_echo_argv_script(tmp.path());
        let (args, egress) = runner_args(tmp.path(), &stub_claude);
        let ready_marker = args.ready_marker.clone();

        let runner_task = tokio::spawn(async move {
            let _ = run_sandbox_runner(args).await;
        });

        wait_for_ready(&ready_marker, &egress, Duration::from_secs(15)).await;

        let mut client = connect_sandbox_client(&ready_marker)
            .await
            .expect("connect sandbox grpc");
        let (host_tx, host_rx) = tokio::sync::mpsc::channel(64);
        let mut session = client
            .session_channel(ReceiverStream::new(host_rx))
            .await
            .expect("SessionChannel must open")
            .into_inner();

        host_tx
            .send(SessionFrame {
                payload: Some(SessionPayload::SubscribeTerminal(SubscribeTerminal {
                    session_id: SESSION_ID.to_string(),
                    terminal_id: "main".to_string(),
                    initial_cols: 80,
                    initial_rows: 24,
                })),
            })
            .await
            .expect("subscribe");

        let host_tx_poll = host_tx.clone();
        let egress_for_reader = egress.clone();
        let reader = tokio::spawn(async move {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while tokio::time::Instant::now() < deadline {
                if let Some(Ok(frame)) =
                    tokio::time::timeout(Duration::from_millis(200), session.next())
                        .await
                        .ok()
                        .flatten()
                {
                    if let Some(SessionPayload::TerminalOutput(out)) = frame.payload {
                        if String::from_utf8_lossy(&out.data).contains("ARGV:") {
                            return true;
                        }
                    }
                }
                let _ = host_tx_poll
                    .send(SessionFrame {
                        payload: Some(SessionPayload::HostPoll(HostPoll {})),
                    })
                    .await;
            }
            false
        });

        // When
        host_tx
            .send(SessionFrame {
                payload: Some(SessionPayload::TerminalInput(SandboxInput {
                    session_id: SESSION_ID.to_string(),
                    terminal_id: "main".to_string(),
                    data: b"hello-sandbox\n".to_vec(),
                })),
            })
            .await
            .expect("terminal input");

        // Then
        let saw_argv = reader.await.expect("reader task");
        assert!(
            saw_argv,
            "SessionChannel must stream stub claude PTY output\n{}",
            format_egress_logs(&egress_for_reader)
        );

        stop_runner(runner_task).await;
    })
    .await
    .expect("sandbox_runner_session_channel_streams_pty_output_and_accepts_input timed out");
}

/// **sandbox_runner_session_channel_tool_exec_round_trips**: MCP tool IPC queues a call; host
/// polls on SessionChannel, executes on the fake daemon side, and IPC returns the response.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sandbox_runner_session_channel_tool_exec_round_trips() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        // Given
        let tmp = tempfile::tempdir().unwrap();
        let stub_claude = write_echo_argv_script(tmp.path());
        let (args, egress) = runner_args(tmp.path(), &stub_claude);
        let ready_marker = args.ready_marker.clone();
        let tool_ipc = args.tool_ipc_socket.clone();
        std::env::set_var("TDDY_SANDBOX_TOOL_IPC", &tool_ipc);

        let runner_task = tokio::spawn(async move {
            let _ = run_sandbox_runner(args).await;
        });

        wait_for_ready(&ready_marker, &egress, Duration::from_secs(15)).await;

        let _reader = open_host_session_channel(&ready_marker).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        // When
        let ipc_result = tokio::time::timeout(
            Duration::from_secs(10),
            tddy_tools::session_tool_client::dispatch_session_tool(
                "Read",
                serde_json::json!({"path": "README.md"}),
            ),
        )
        .await
        .unwrap_or_else(|_| {
            panic!(
                "tool IPC dispatch timed out\n{}",
                format_egress_logs(&egress)
            )
        });

        // Then — dispatch_session_tool returns the tool result JSON directly on success
        let parsed: serde_json::Value =
            serde_json::from_str(&ipc_result).expect("valid json response");
        assert_eq!(
            parsed.get("path").and_then(|v| v.as_str()),
            Some("Read"),
            "tool IPC must return host response: {parsed}\n{}",
            format_egress_logs(&egress)
        );

        stop_runner(runner_task).await;
    })
    .await
    .expect("sandbox_runner_session_channel_tool_exec_round_trips timed out");
}

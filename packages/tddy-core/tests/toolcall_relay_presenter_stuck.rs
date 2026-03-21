//! When the Unix relay accepts a submit but the presenter never calls [`Presenter::poll_tool_calls`],
//! the listener must still complete the connection with an error response so `tddy-tools submit`
//! unblocks. Otherwise the agent and relay hang (`[wait] waiting for presenter response...`).

use serde_json::json;
use serde_json::Value;
use std::time::Duration;
use tddy_core::toolcall::start_toolcall_listener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;

#[tokio::test]
#[cfg(unix)]
async fn relay_responds_with_error_when_presenter_never_polls_submit() {
    let (socket_path, _hold_tool_rx) = start_toolcall_listener().expect("start listener");

    let path = socket_path.clone();
    let client = tokio::spawn(async move {
        let mut stream = UnixStream::connect(path).await.expect("connect");
        let line = json!({
            "type": "submit",
            "goal": "plan",
            "data": {"goal": "plan", "prd": "# x"}
        })
        .to_string();
        stream.write_all(line.as_bytes()).await.expect("write body");
        stream.write_all(b"\n").await.expect("write newline");
        stream.flush().await.expect("flush");
        let mut reader = tokio::io::BufReader::new(stream);
        let mut buf = String::new();
        reader
            .read_line(&mut buf)
            .await
            .expect("read response line");
        buf
    });

    let deadline = Duration::from_secs(2);
    let wrapped = timeout(deadline, client).await;
    assert!(
        wrapped.is_ok(),
        "relay must return a line within {:?} when presenter never polls (stuck case); client hung waiting for response",
        deadline
    );
    let line = wrapped.unwrap().expect("join client task");
    let v: Value = serde_json::from_str(line.trim()).expect("response is JSON");
    assert_eq!(v["status"], "error", "expected error status, got: {}", line);
    let msg = v["message"]
        .as_str()
        .expect("error response must include message");
    assert!(
        !msg.is_empty(),
        "error message must be non-empty for stuck presenter case"
    );
}

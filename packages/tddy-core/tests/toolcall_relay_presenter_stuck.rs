//! Submit is acknowledged on the wire immediately after storing results; the presenter only
//! receives an activity-log notification. If the presenter never polls, `tddy-tools` still gets
//! `{"status":"ok",...}` without waiting on the UI loop.
//!
//! End-to-end CLI coverage: `packages/tddy-tools/tests/submit_relay_no_poll.rs` (same invariant).

use serde_json::json;
use serde_json::Value;
use std::time::Duration;
use tddy_core::toolcall::start_toolcall_listener;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;

#[tokio::test]
#[cfg(unix)]
async fn relay_accepts_submit_when_presenter_never_polls() {
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
    assert_eq!(v["status"], "ok", "expected ok status, got: {}", line);
    assert_eq!(v["goal"], "plan");
}

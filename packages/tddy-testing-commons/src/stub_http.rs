//! A loopback HTTP endpoint that answers every request `200 OK`.
//!
//! Tests that would otherwise need a real inference server (the specialized-agent warm-up gate
//! probes `POST {base_url}/v1/chat/completions` and waits for a `200`) point the agent's
//! `base_url` at one of these instead. The suite then depends on nothing outside the repo.
//!
//! Two details make it reliable where an ad-hoc `TcpListener` + `write_all` is not:
//!
//! - It binds `127.0.0.1:0` and reports the port the kernel picked, so two tests running at once
//!   can never collide on a hardcoded port.
//! - It **reads the whole request — headers and `Content-Length` body — before replying.** A
//!   server that replies and closes while the client is still writing gets that write reset;
//!   reqwest reports it as a connection error, which the warm-up gate classifies as transient and
//!   retries until its budget elapses. Draining first is what makes the `200` dependable under
//!   load.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

/// A minimal, well-formed chat-completion response — enough for a caller that parses the body
/// rather than only checking the status.
const READY_BODY: &str = r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#;

/// A running stub endpoint. Dropping it stops accepting connections.
pub struct StubHttpEndpoint {
    port: u16,
    served: Arc<AtomicUsize>,
    accepting: JoinHandle<()>,
}

impl StubHttpEndpoint {
    /// The loopback port the kernel assigned.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The `http://127.0.0.1:<port>` prefix to hand to whatever is under test.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// How many requests have been answered — lets a test assert the code under test actually
    /// probed the endpoint, rather than inferring it from the absence of an error.
    pub fn served_requests(&self) -> usize {
        self.served.load(Ordering::SeqCst)
    }
}

impl Drop for StubHttpEndpoint {
    fn drop(&mut self) {
        self.accepting.abort();
    }
}

/// A stub endpoint on an ephemeral loopback port that answers every request `200 OK`.
pub async fn a_stub_http_endpoint_answering_ok() -> StubHttpEndpoint {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a loopback stub endpoint");
    let port = listener
        .local_addr()
        .expect("stub endpoint local address")
        .port();

    let served = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&served);
    let accepting = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let counted = Arc::clone(&counter);
            tokio::spawn(async move {
                answer_ok(stream, counted).await;
            });
        }
    });

    StubHttpEndpoint {
        port,
        served,
        accepting,
    }
}

/// A stub endpoint that accepts the connection, reads the whole request, and then **never
/// answers** — the failure mode a deadline exists for.
///
/// A refused connection or a closed socket produces an error on its own; only an endpoint that
/// takes the request and goes quiet distinguishes a client that has a timeout from one that waits
/// forever. Requests are counted, so a test can assert the call was actually made rather than
/// inferring it from elapsed time.
pub async fn a_stub_http_endpoint_that_never_answers() -> StubHttpEndpoint {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a loopback stub endpoint");
    let port = listener
        .local_addr()
        .expect("stub endpoint local address")
        .port();

    let served = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&served);
    let accepting = tokio::spawn(async move {
        // Every accepted stream is kept alive here, so the client sees an open connection with no
        // response rather than a peer that hung up.
        let mut held = Vec::new();
        while let Ok((mut stream, _)) = listener.accept().await {
            if drain_request(&mut stream).await.is_some() {
                counter.fetch_add(1, Ordering::SeqCst);
            }
            held.push(stream);
        }
    });

    StubHttpEndpoint {
        port,
        served,
        accepting,
    }
}

/// Drain one request, then reply `200 OK` and close.
async fn answer_ok(mut stream: TcpStream, served: Arc<AtomicUsize>) {
    if drain_request(&mut stream).await.is_none() {
        return;
    }
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{READY_BODY}",
        READY_BODY.len()
    );
    if stream.write_all(response.as_bytes()).await.is_ok() {
        served.fetch_add(1, Ordering::SeqCst);
        let _ = stream.flush().await;
    }
}

/// Read headers and the declared body. `None` if the peer hung up mid-request.
async fn drain_request(stream: &mut TcpStream) -> Option<()> {
    let mut buffered = Vec::new();
    let mut chunk = [0_u8; 4096];

    let head_len = loop {
        if let Some(at) = header_end(&buffered) {
            break at;
        }
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffered.extend_from_slice(&chunk[..read]);
    };

    let headers = String::from_utf8_lossy(&buffered[..head_len]).into_owned();
    let want = head_len + declared_body_len(&headers);
    while buffered.len() < want {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffered.extend_from_slice(&chunk[..read]);
    }

    Some(())
}

/// Byte offset just past the blank line ending the request head, once it has arrived.
fn header_end(buffered: &[u8]) -> Option<usize> {
    buffered
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| at + 4)
}

/// The `Content-Length` the request declared; `0` when it declared none.
fn declared_body_len(headers: &str) -> usize {
    let lowercased = headers.to_ascii_lowercase();
    lowercased
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Comfortably more than one socket read, so a stub that replied without draining would reset
    /// the client mid-write.
    const A_BODY_LARGER_THAN_ONE_SOCKET_READ: usize = 256 * 1024;

    /// A JSON probe body of a chosen size, shaped like the warm-up gate's wake-up request.
    fn a_probe_body(size: usize) -> String {
        format!(r#"{{"model":"a-model","prompt":"{}"}}"#, "x".repeat(size))
    }

    fn a_probe_to(endpoint: &StubHttpEndpoint, body: String) -> reqwest::RequestBuilder {
        reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", endpoint.base_url()))
            .header("content-type", "application/json")
            .body(body)
    }

    #[tokio::test]
    async fn answers_a_posted_probe_with_200() {
        // Given a stub endpoint on an ephemeral loopback port
        let endpoint = a_stub_http_endpoint_answering_ok().await;

        // When probing it the way the warm-up gate does
        let response = a_probe_to(&endpoint, a_probe_body(4))
            .send()
            .await
            .expect("probe the stub endpoint");

        // Then it answers 200 and reports having served the probe
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(endpoint.served_requests(), 1);
    }

    #[tokio::test]
    async fn answers_a_probe_whose_body_spans_several_socket_reads() {
        // Given a stub endpoint and a probe body far larger than a single read
        let endpoint = a_stub_http_endpoint_answering_ok().await;

        // When posting the whole body
        let response = a_probe_to(&endpoint, a_probe_body(A_BODY_LARGER_THAN_ONE_SOCKET_READ))
            .send()
            .await
            .expect("probe the stub endpoint with a large body");

        // Then the stub drained it all and still answered 200
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(endpoint.served_requests(), 1);
    }

    #[tokio::test]
    async fn reports_the_ephemeral_port_it_bound_in_its_base_url() {
        // Given a stub endpoint
        let endpoint = a_stub_http_endpoint_answering_ok().await;

        // Then its base URL names the loopback port the kernel assigned
        assert_eq!(
            endpoint.base_url(),
            format!("http://127.0.0.1:{}", endpoint.port())
        );
    }

    #[test]
    fn reads_the_content_length_a_request_declared() {
        // Given a request head written with the casing reqwest uses
        let headers = "POST /v1/chat/completions HTTP/1.1\r\ncontent-length: 4096\r\n\r\n";

        // Then the declared body length is read back exactly
        assert_eq!(declared_body_len(headers), 4096);
    }

    #[test]
    fn treats_a_request_without_a_content_length_as_bodyless() {
        // Given a request head declaring no body
        let headers = "GET /health HTTP/1.1\r\nhost: 127.0.0.1\r\n\r\n";

        // Then nothing is waited for beyond the head
        assert_eq!(declared_body_len(headers), 0);
    }
}

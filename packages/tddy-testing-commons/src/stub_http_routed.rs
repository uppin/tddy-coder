//! A loopback HTTP endpoint that answers **per path** and records what it was asked.
//!
//! [`stub_http`](crate::stub_http) answers every request with the same `200` — right for a
//! reachability probe, useless for pinning a wire contract. A provider client talks to several
//! endpoints (`/api/tags`, `/api/show`, `/api/ps`, `/api/generate`, `/v1/chat/completions`) and the
//! request *bodies* are half the contract, so this variant:
//!
//! - routes by path, answering `404` for anything unrouted, so an unexpected call fails loudly
//!   rather than silently succeeding against a catch-all `200`;
//! - records every `(path, headers, body)` so a test can assert what was actually sent — including
//!   the `Authorization` a provider client is supposed to carry;
//! - drains the whole request (headers + declared `Content-Length` body) before replying, for the
//!   same reason [`stub_http`] does — replying mid-write resets the client's socket.
//!
//! A route answers either the *same* body to every request
//! ([`a_stub_http_endpoint_routing`]) or a *sequence* of bodies, one per request
//! ([`a_stub_http_endpoint_replying_in_sequence`]) — which is what a multi-round conversation
//! needs, since replaying one body forever turns "the model answered on its second turn" into "the
//! model asked for the same tool again".
//!
//! It binds `127.0.0.1:0`, so concurrent tests never collide on a port.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// One request the stub answered: the path it hit, the headers it carried, and its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedRequest {
    pub path: String,
    /// Every header line, in the order sent, names lower-cased. A repeated name appears repeatedly.
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl RecordedRequest {
    /// The first value sent for `name` (case-insensitive), or `None` when the header was absent.
    pub fn header(&self, name: &str) -> Option<&str> {
        let wanted = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(header, _)| header == &wanted)
            .map(|(_, value)| value.as_str())
    }
}

/// What one routed path answers with.
#[derive(Clone, Debug)]
enum Route {
    /// The same body to every request on this path.
    Constant(String),
    /// The Nth request gets the Nth body; once the list is exhausted the path answers `500`, so an
    /// unexpected extra round trip fails the test rather than silently repeating the last answer.
    Sequence(Vec<String>),
}

/// A running path-routed stub. Dropping it stops accepting connections.
pub struct RoutedStubHttpEndpoint {
    port: u16,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    accepting: JoinHandle<()>,
}

impl RoutedStubHttpEndpoint {
    /// The `http://127.0.0.1:<port>` prefix to hand to whatever is under test.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Every request answered so far, in order.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().expect("stub request log").clone()
    }

    /// The paths requested so far, in order.
    pub fn paths(&self) -> Vec<String> {
        self.requests().into_iter().map(|r| r.path).collect()
    }

    /// Every request made to `path`, in order. Panics if there were none — a test that asserts on
    /// a request it never received should fail loudly, not compare against an empty list.
    pub fn requests_to(&self, path: &str) -> Vec<RecordedRequest> {
        let matching: Vec<RecordedRequest> = self
            .requests()
            .into_iter()
            .filter(|r| r.path == path)
            .collect();
        assert!(!matching.is_empty(), "no request was made to {path}");
        matching
    }

    /// The body of the first request to `path`. Panics if no such request was made — a test that
    /// asserts on a body it never received should fail loudly, not compare against an empty string.
    pub fn body_for(&self, path: &str) -> String {
        self.requests_to(path).swap_remove(0).body
    }

    /// The body of the first request to `path`, parsed as JSON.
    pub fn json_body_for(&self, path: &str) -> serde_json::Value {
        parse_json(path, &self.body_for(path))
    }

    /// The bodies of every request to `path`, in order, parsed as JSON — for a path a client hits
    /// more than once in one exchange.
    pub fn json_bodies_for(&self, path: &str) -> Vec<serde_json::Value> {
        self.requests_to(path)
            .into_iter()
            .map(|request| parse_json(path, &request.body))
            .collect()
    }

    /// The first value the first request to `path` sent for the `name` header, or `None` when it
    /// sent none.
    pub fn header_for(&self, path: &str, name: &str) -> Option<String> {
        self.requests_to(path)
            .swap_remove(0)
            .header(name)
            .map(str::to_string)
    }
}

fn parse_json(path: &str, body: &str) -> serde_json::Value {
    serde_json::from_str(body)
        .unwrap_or_else(|e| panic!("request body to {path} was not json ({e}): {body}"))
}

impl Drop for RoutedStubHttpEndpoint {
    fn drop(&mut self) {
        self.accepting.abort();
    }
}

/// A stub serving `routes` (path → the JSON body that path always answers with) on an ephemeral
/// loopback port.
///
/// ```ignore
/// let stub = a_stub_http_endpoint_routing(&[("/api/ps", r#"{"models":[]}"#)]).await;
/// ```
pub async fn a_stub_http_endpoint_routing(routes: &[(&str, &str)]) -> RoutedStubHttpEndpoint {
    serve(
        routes
            .iter()
            .map(|(path, body)| (path.to_string(), Route::Constant(body.to_string())))
            .collect(),
    )
    .await
}

/// A stub whose routes answer **one body per request**: the Nth request to a path gets that path's
/// Nth body, and a further request gets `500`.
///
/// ```ignore
/// let stub = a_stub_http_endpoint_replying_in_sequence(&[(
///     "/v1/chat/completions",
///     &[COMPLETION_CALLING_READ, COMPLETION_SAYING_HELLO],
/// )])
/// .await;
/// ```
pub async fn a_stub_http_endpoint_replying_in_sequence(
    routes: &[(&str, &[&str])],
) -> RoutedStubHttpEndpoint {
    serve(
        routes
            .iter()
            .map(|(path, bodies)| {
                (
                    path.to_string(),
                    Route::Sequence(bodies.iter().map(|body| body.to_string()).collect()),
                )
            })
            .collect(),
    )
    .await
}

async fn serve(table: HashMap<String, Route>) -> RoutedStubHttpEndpoint {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind a loopback routed stub endpoint");
    let port = listener
        .local_addr()
        .expect("routed stub local address")
        .port();

    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    // One shared cursor per path, so sequenced routes advance across connections (the clients under
    // test send `Connection: close`, so every request arrives on a new one).
    let served: Arc<Mutex<HashMap<String, usize>>> = Arc::new(Mutex::new(HashMap::new()));
    let table = Arc::new(table);

    let accepting = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let recorded = Arc::clone(&recorded);
            let table = Arc::clone(&table);
            let served = Arc::clone(&served);
            tokio::spawn(async move {
                serve_one(stream, table, served, recorded).await;
            });
        }
    });

    RoutedStubHttpEndpoint {
        port,
        requests,
        accepting,
    }
}

async fn serve_one(
    mut stream: TcpStream,
    routes: Arc<HashMap<String, Route>>,
    served: Arc<Mutex<HashMap<String, usize>>>,
    recorded: Arc<Mutex<Vec<RecordedRequest>>>,
) {
    let Some(request) = read_request(&mut stream).await else {
        return;
    };
    // A path carrying a query string still routes on the path alone.
    let routed = request
        .path
        .split('?')
        .next()
        .unwrap_or(&request.path)
        .to_string();
    recorded.lock().expect("stub request log").push(request);

    let ordinal = {
        let mut served = served.lock().expect("stub route cursors");
        let count = served.entry(routed.clone()).or_insert(0);
        let ordinal = *count;
        *count += 1;
        ordinal
    };

    let response = match routes.get(&routed) {
        Some(Route::Constant(json)) => ok_with(json),
        Some(Route::Sequence(bodies)) => match bodies.get(ordinal) {
            Some(json) => ok_with(json),
            None => exhausted(&routed, bodies.len()),
        },
        None => {
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
        }
    };
    if stream.write_all(response.as_bytes()).await.is_ok() {
        let _ = stream.flush().await;
    }
}

fn ok_with(json: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
        json.len()
    )
}

/// The answer to a request past the end of a sequenced route: a `500` naming the exhaustion, so the
/// client under test reports a failure a test can read rather than looping on a repeated body.
fn exhausted(path: &str, scripted: usize) -> String {
    let body = format!("the stub has only {scripted} scripted response(s) for {path}");
    format!(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Read the request path, headers and declared body. `None` if the peer hung up mid-request.
async fn read_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    let mut buffered = Vec::new();
    let mut chunk = [0_u8; 4096];

    let head_len = loop {
        if let Some(at) = buffered.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffered.extend_from_slice(&chunk[..read]);
    };

    let head = String::from_utf8_lossy(&buffered[..head_len]).into_owned();
    let path = head.lines().next()?.split_whitespace().nth(1)?.to_string();
    let headers = parse_headers(&head);

    let want = head_len + declared_body_len(&head);
    while buffered.len() < want {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffered.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&buffered[head_len..want]).into_owned();
    Some(RecordedRequest {
        path,
        headers,
        body,
    })
}

/// The header lines of a request head, names lower-cased and values trimmed. The request line and
/// the blank terminator are not headers, so they are skipped.
fn parse_headers(head: &str) -> Vec<(String, String)> {
    head.lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect()
}

/// The `Content-Length` the request declared; `0` when it declared none.
fn declared_body_len(head: &str) -> usize {
    head.to_ascii_lowercase()
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// POST `body` to `path` on the stub and hand back the status and the response text.
    async fn post_to(stub: &RoutedStubHttpEndpoint, path: &str, body: &str) -> (u16, String) {
        let response = reqwest::Client::new()
            .post(format!("{}{path}", stub.base_url()))
            .header("content-type", "application/json")
            .header("authorization", "Bearer fw-secret-key")
            .body(body.to_string())
            .send()
            .await
            .expect("post to the routed stub");
        let status = response.status().as_u16();
        (status, response.text().await.expect("a response body"))
    }

    #[tokio::test]
    async fn answers_a_routed_path_with_its_body_and_records_the_request() {
        // Given a stub routing one path
        let stub = a_stub_http_endpoint_routing(&[("/api/ps", r#"{"models":[]}"#)]).await;

        // When
        let (status, text) = post_to(&stub, "/api/ps", r#"{"probe":true}"#).await;

        // Then the routed body comes back and the request was recorded verbatim
        assert_eq!(status, 200);
        assert_eq!(text, r#"{"models":[]}"#);
        assert_eq!(stub.paths(), vec!["/api/ps".to_string()]);
        assert_eq!(stub.json_body_for("/api/ps")["probe"], true);
    }

    #[tokio::test]
    async fn records_the_headers_a_request_carried() {
        // Given
        let stub = a_stub_http_endpoint_routing(&[("/v1/models", r#"{"data":[]}"#)]).await;

        // When
        post_to(&stub, "/v1/models", r#"{}"#).await;

        // Then — a credential the client was supposed to send is assertable
        assert_eq!(
            stub.header_for("/v1/models", "authorization"),
            Some("Bearer fw-secret-key".to_string())
        );
    }

    #[tokio::test]
    async fn answers_a_constant_route_with_the_same_body_on_every_request() {
        // Given
        let stub = a_stub_http_endpoint_routing(&[("/api/show", r#"{"capabilities":[]}"#)]).await;

        // When the same path is hit twice
        let first = post_to(&stub, "/api/show", r#"{"model":"qwen3:32b"}"#).await;
        let second = post_to(&stub, "/api/show", r#"{"model":"nomic-embed-text"}"#).await;

        // Then
        assert_eq!(first, (200, r#"{"capabilities":[]}"#.to_string()));
        assert_eq!(second, (200, r#"{"capabilities":[]}"#.to_string()));
    }

    #[tokio::test]
    async fn answers_a_sequenced_route_with_one_body_per_request_in_order() {
        // Given a path scripted with two answers
        let stub = a_stub_http_endpoint_replying_in_sequence(&[(
            "/v1/chat/completions",
            &[r#"{"turn":"first"}"#, r#"{"turn":"second"}"#],
        )])
        .await;

        // When
        let first = post_to(&stub, "/v1/chat/completions", r#"{"round":1}"#).await;
        let second = post_to(&stub, "/v1/chat/completions", r#"{"round":2}"#).await;

        // Then each request got its own scripted answer
        assert_eq!(first, (200, r#"{"turn":"first"}"#.to_string()));
        assert_eq!(second, (200, r#"{"turn":"second"}"#.to_string()));
        assert_eq!(
            stub.json_bodies_for("/v1/chat/completions"),
            vec![
                serde_json::json!({"round": 1}),
                serde_json::json!({"round": 2})
            ]
        );
    }

    #[tokio::test]
    async fn answers_a_request_past_the_end_of_a_sequence_with_500() {
        // Given a path scripted with exactly one answer
        let stub = a_stub_http_endpoint_replying_in_sequence(&[(
            "/v1/chat/completions",
            &[r#"{"turn":"only"}"#],
        )])
        .await;
        post_to(&stub, "/v1/chat/completions", r#"{"round":1}"#).await;

        // When one more request arrives than the script covers
        let (status, body) = post_to(&stub, "/v1/chat/completions", r#"{"round":2}"#).await;

        // Then — a loud failure, never a silent replay of the last answer
        assert_eq!(status, 500);
        assert_eq!(
            body,
            "the stub has only 1 scripted response(s) for /v1/chat/completions"
        );
    }

    #[tokio::test]
    async fn answers_an_unrouted_path_with_404_so_an_unexpected_call_is_visible() {
        // Given a stub routing nothing
        let stub = a_stub_http_endpoint_routing(&[]).await;

        // When
        let response = reqwest::Client::new()
            .get(format!("{}/api/tags", stub.base_url()))
            .send()
            .await
            .expect("get from the routed stub");

        // Then
        assert_eq!(response.status().as_u16(), 404);
    }
}

//! The OAuth callback, for a host application that serves no web origin.
//!
//! GitHub finishes a sign-in by redirecting a browser, so the flow needs somewhere to land. The
//! daemon's own answer is its web server — which this application deliberately does not run. What
//! it runs instead is this: a loopback listener that serves exactly one path, `/auth/callback`,
//! takes the `code` and `state` off the query, and hands them to the window so the dashboard's
//! existing callback route can complete the exchange.
//!
//! Two properties matter and are enforced here rather than assumed:
//!
//! * **Loopback only.** The callback carries an authorization code. Binding anything but
//!   `127.0.0.1` would put it on the network, and the address an operator configured for a *served*
//!   daemon (`WEB_PUBLIC_URL`, a LAN address) is not reachable from the machine the browser is on
//!   in the first place.
//! * **One path.** This is not the daemon's web server returning; nothing else is routed, and no
//!   RPC surface is reachable through it.

use std::collections::HashMap;
use std::net::SocketAddr;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// What a callback request carried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

/// The loopback callback URL for `port`, as the daemon should hand it to GitHub.
pub fn callback_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/auth/callback")
}

/// Parse the `code` and `state` out of a callback request's target (`/auth/callback?...`).
///
/// Returns `None` for any other path, and for a callback missing either parameter — a request
/// without both cannot complete a sign-in, and guessing at one would send the dashboard to a route
/// that fails less clearly than not going at all.
pub fn parse_callback(request_target: &str) -> Option<CallbackParams> {
    let (path, query) = request_target.split_once('?')?;
    if path != "/auth/callback" {
        return None;
    }
    let params: HashMap<&str, String> = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k, percent_decode(v)))
        .collect();
    Some(CallbackParams {
        code: params.get("code")?.clone(),
        state: params.get("state")?.clone(),
    })
}

/// Decode the `%XX` escapes and `+` a browser puts in a query value.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match u8::from_str_radix(&value[i + 1..i + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    i += 3;
                }
                Err(_) => {
                    out.push(b'%');
                    i += 1;
                }
            },
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The page the browser is left on once the code has been taken.
pub const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Signed in</title>\
<body style=\"font:16px system-ui;padding:3rem;text-align:center\">\
<p>Signed in. You can close this tab and return to Tddy Desktop.</p>";

/// Serve `/auth/callback` on `addr` until a request carries a usable `code` and `state`.
///
/// Returns as soon as one does, so the socket is open for the length of a sign-in rather than the
/// life of the process. Anything else — a favicon probe, a stray request, a callback missing a
/// parameter — is answered `404` and the listener keeps waiting, because closing on the first
/// wrong request would end the sign-in the operator is in the middle of.
pub async fn await_callback(addr: SocketAddr) -> std::io::Result<CallbackParams> {
    let listener = TcpListener::bind(addr).await?;
    log::info!("[tddy-desktop] waiting for the sign-in callback on http://{addr}/auth/callback");
    loop {
        let (stream, _) = listener.accept().await?;
        if let Some(params) = serve_one(stream).await? {
            return Ok(params);
        }
    }
}

/// Answer one connection. `Some` when it was the callback we are waiting for.
async fn serve_one(stream: TcpStream) -> std::io::Result<Option<CallbackParams>> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    // "GET /auth/callback?code=… HTTP/1.1"
    let target = request_line.split_whitespace().nth(1).unwrap_or_default();
    let params = parse_callback(target);

    let response = match &params {
        Some(_) => format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            DONE_PAGE.len(),
            DONE_PAGE
        ),
        None => "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string(),
    };
    reader.get_mut().write_all(response.as_bytes()).await?;
    reader.get_mut().flush().await?;
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn reads_the_code_and_state_a_callback_carries() {
        // Given the callback GitHub redirects a browser to
        let target = "/auth/callback?code=test-code&state=eea0e57d-910e";

        // When it is parsed
        let params = parse_callback(target);

        // Then both values the exchange needs come back
        assert_eq!(
            params,
            Some(CallbackParams {
                code: "test-code".to_string(),
                state: "eea0e57d-910e".to_string(),
            })
        );
    }

    #[test]
    fn decodes_the_escapes_a_browser_puts_in_a_value() {
        // Given a callback whose state was percent-encoded
        let target = "/auth/callback?code=a%2Fb&state=x%3Dy+z";

        // When it is parsed
        let params = parse_callback(target).expect("a callback with both parameters");

        // Then the values are what the provider sent, not what the wire carried
        assert_eq!(params.code, "a/b");
        assert_eq!(params.state, "x=y z");
    }

    #[rstest]
    #[case::another_path("/anything?code=c&state=s")]
    #[case::no_query("/auth/callback")]
    #[case::no_code("/auth/callback?state=s")]
    #[case::no_state("/auth/callback?code=c")]
    fn reads_nothing_from_a_request_that_cannot_complete_a_sign_in(#[case] target: &str) {
        // Given a request that is not a complete callback

        // When it is parsed
        let params = parse_callback(target);

        // Then nothing is taken from it — a partial callback would send the dashboard to a route
        // that fails less clearly than not going at all
        assert_eq!(params, None);
    }

    #[test]
    fn addresses_the_callback_to_loopback_only() {
        // Given the port the host application listens on for a sign-in

        // When the callback URL is built
        let url = callback_url(8899);

        // Then it names loopback: the code must come back to this process, and an address
        // configured for a served daemon is not reachable from the browser's machine anyway
        assert_eq!(url, "http://127.0.0.1:8899/auth/callback");
    }
}

#[cfg(test)]
mod serving {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};
    use tokio::io::AsyncReadExt;

    /// A loopback address on a port the OS picked, so a test never collides with a real daemon.
    fn a_free_loopback_address() -> SocketAddr {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a free port");
        let addr = listener.local_addr().expect("the bound address");
        drop(listener);
        addr
    }

    /// Make the request a browser would, and read what came back.
    async fn request(addr: SocketAddr, target: &str) -> String {
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        stream
            .write_all(format!("GET {target} HTTP/1.1\r\nhost: localhost\r\n\r\n").as_bytes())
            .await
            .expect("write");
        let mut response = String::new();
        stream.read_to_string(&mut response).await.expect("read");
        response
    }

    #[tokio::test]
    async fn takes_the_code_from_the_callback_a_browser_is_redirected_to() {
        // Given the host application waiting for a sign-in to come back
        let addr = a_free_loopback_address();
        let waiting = tokio::spawn(await_callback(addr));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // When the browser lands on the callback
        let response = request(addr, "/auth/callback?code=test-code&state=abc").await;

        // Then the code is handed to the application, and the browser is told it can go
        let params = waiting.await.expect("join").expect("the callback");
        assert_eq!(params.code, "test-code");
        assert_eq!(params.state, "abc");
        assert!(response.starts_with("HTTP/1.1 200 OK"), "was: {response}");
        assert!(response.contains("close this tab"), "was: {response}");
    }

    #[tokio::test]
    async fn keeps_waiting_through_a_request_that_is_not_the_callback() {
        // Given the host application waiting for a sign-in
        let addr = a_free_loopback_address();
        let waiting = tokio::spawn(await_callback(addr));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // When something else asks for something else — a browser probing for a favicon, say
        let refused = request(addr, "/favicon.ico").await;

        // Then it is refused and the sign-in is still expected: closing on the first wrong request
        // would end the sign-in the operator is in the middle of
        assert!(refused.starts_with("HTTP/1.1 404"), "was: {refused}");
        let response = request(addr, "/auth/callback?code=late&state=s").await;
        assert!(response.starts_with("HTTP/1.1 200 OK"), "was: {response}");
        assert_eq!(
            waiting.await.expect("join").expect("the callback").code,
            "late"
        );
    }
}

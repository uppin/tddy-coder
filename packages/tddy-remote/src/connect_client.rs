//! Minimal Connect unary client (protobuf) for `remote_sandbox.v1.RemoteSandboxService`.

use prost::Message;
use reqwest::Url;

const SERVICE: &str = "remote_sandbox.v1.RemoteSandboxService";

pub(crate) async fn unary_proto(
    base_url: &str,
    method: &str,
    body: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let base = base_url.trim_end_matches('/');
    let url_s = format!("{base}/rpc/{SERVICE}/{method}");
    let url = Url::parse(&url_s).map_err(|e| format!("invalid URL {url_s}: {e}"))?;
    log::debug!("Connect unary POST {url}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(url)
        .header("Content-Type", "application/proto")
        .header("Connect-Protocol-Version", "1")
        .body(body)
        .send()
        .await
        .map_err(|e| format!("http: {e}"))?;
    let status = resp.status();
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!(
            "Connect RPC {method} HTTP {status} body_len={}",
            bytes.len()
        ));
    }
    Ok(bytes.to_vec())
}

pub(crate) fn encode<M: Message>(m: &M) -> Vec<u8> {
    let mut v = Vec::new();
    m.encode(&mut v).expect("prost encode");
    v
}

pub(crate) fn decode<M: Message + Default>(bytes: &[u8]) -> Result<M, prost::DecodeError> {
    M::decode(bytes)
}

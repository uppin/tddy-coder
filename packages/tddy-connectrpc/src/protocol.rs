//! Connect protocol constants and validation.

/// Connect protocol version.
pub const CONNECT_PROTOCOL_VERSION: &str = "1";

/// Header name for Connect protocol version.
pub const CONNECT_PROTOCOL_VERSION_HEADER: &str = "connect-protocol-version";

/// Content-Type for unary protobuf.
pub const CONTENT_TYPE_PROTO: &str = "application/proto";

/// Content-Type for streaming protobuf.
pub const CONTENT_TYPE_CONNECT_PROTO: &str = "application/connect+proto";

/// Content-Type for streaming JSON.
pub const CONTENT_TYPE_CONNECT_JSON: &str = "application/connect+json";

/// Protocol variant from Content-Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestProtocol {
    ConnectUnaryProto,
    ConnectUnaryJson,
    ConnectStreamProto,
    ConnectStreamJson,
    Unknown,
}

impl RequestProtocol {
    pub fn from_content_type(content_type: &str) -> Self {
        let ct = content_type.split(';').next().unwrap_or("").trim();
        if ct.starts_with("application/connect+proto") {
            Self::ConnectStreamProto
        } else if ct.starts_with("application/connect+json") {
            Self::ConnectStreamJson
        } else if ct.starts_with("application/proto") {
            Self::ConnectUnaryProto
        } else if ct.starts_with("application/json") {
            Self::ConnectUnaryJson
        } else {
            Self::Unknown
        }
    }

    pub fn is_proto(&self) -> bool {
        matches!(self, Self::ConnectUnaryProto | Self::ConnectStreamProto)
    }

    pub fn is_streaming(&self) -> bool {
        matches!(self, Self::ConnectStreamProto | Self::ConnectStreamJson)
    }

    pub fn response_content_type(&self) -> &'static str {
        match self {
            Self::ConnectUnaryProto | Self::ConnectStreamProto => CONTENT_TYPE_PROTO,
            Self::ConnectUnaryJson | Self::ConnectStreamJson => "application/json",
            Self::Unknown => "application/json",
        }
    }

    pub fn streaming_response_content_type(&self) -> &'static str {
        if self.is_proto() {
            CONTENT_TYPE_CONNECT_PROTO
        } else {
            CONTENT_TYPE_CONNECT_JSON
        }
    }
}

/// Validate Connect-Protocol-Version header. Returns error message if invalid.
pub fn validate_protocol_version(headers: &axum::http::HeaderMap) -> Option<&'static str> {
    let version = headers
        .get(CONNECT_PROTOCOL_VERSION_HEADER)
        .and_then(|v| v.to_str().ok());

    match version {
        Some(v) if v == CONNECT_PROTOCOL_VERSION => None,
        Some(_) => Some("connect-protocol-version must be \"1\""),
        None => None,
    }
}

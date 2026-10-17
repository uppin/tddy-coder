//! The loopback HTTP route: Connect-style unary, `POST /rpc/askpass.AskpassService/Prompt` with a
//! protobuf body, as the daemon's other callers speak.

use std::sync::Arc;

use axum::body::Bytes;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use prost::Message;
use tddy_service::proto::askpass::{AskpassRequest, AskpassResponse, Outcome, PROMPT_PATH};

use super::endpoint::Core;
use super::outcome::AskpassOutcome;

pub(super) fn router(core: Arc<Core>) -> Router {
    Router::new()
        .route(PROMPT_PATH, post(move |body: Bytes| prompt(core, body)))
        .fallback(|| async { StatusCode::NOT_FOUND })
}

async fn prompt(core: Arc<Core>, body: Bytes) -> Response {
    let Ok(request) = AskpassRequest::decode(body) else {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"code":"invalid_argument","message":"malformed AskpassRequest"}"#,
        )
            .into_response();
    };
    let reply = core.ask(&request.token, &request.prompt).await;
    let response = match reply.outcome {
        // The answer rides in the response for OUTCOME_ANSWER and in no other.
        AskpassOutcome::Answer(answer) => AskpassResponse {
            outcome: Outcome::Answer as i32,
            answer: answer.as_bytes().to_vec(),
            reason: String::new(),
            previous_answer_rejected: reply.previous_answer_rejected,
        },
        AskpassOutcome::Refused(reason) => AskpassResponse {
            outcome: Outcome::Refused as i32,
            answer: Vec::new(),
            reason,
            previous_answer_rejected: false,
        },
        AskpassOutcome::Expired => AskpassResponse {
            outcome: Outcome::Expired as i32,
            answer: Vec::new(),
            reason: String::new(),
            previous_answer_rejected: false,
        },
    };
    (
        [(header::CONTENT_TYPE, "application/proto")],
        response.encode_to_vec(),
    )
        .into_response()
}

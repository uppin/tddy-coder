//! gRPC client to child `tddy-coder` [`tddy_service::gen::presenter_intent_client::PresenterIntentClient`].

use tddy_service::gen::presenter_intent_client::PresenterIntentClient;
use tddy_service::gen::{
    AnswerClarificationMultiSelectRequest, AnswerClarificationSelectRequest,
    AnswerClarificationTextRequest, EmptyPresenterIntent, SubmitFeatureTextRequest,
};
use tonic::transport::Endpoint;

async fn connect_presenter_intent(
    grpc_port: u16,
) -> anyhow::Result<PresenterIntentClient<tonic::transport::Channel>> {
    let uri = format!("http://127.0.0.1:{}", grpc_port);
    let channel = Endpoint::from_shared(uri.clone())?.connect().await?;
    Ok(PresenterIntentClient::new(channel))
}

/// Submit feature description to the running presenter (same port as PresenterObserver).
pub async fn submit_feature_text_localhost(grpc_port: u16, text: &str) -> anyhow::Result<()> {
    let mut client = connect_presenter_intent(grpc_port).await?;
    client
        .submit_feature_text(SubmitFeatureTextRequest {
            text: text.to_string(),
        })
        .await?;
    Ok(())
}

pub async fn approve_session_document_localhost(grpc_port: u16) -> anyhow::Result<()> {
    let mut client = connect_presenter_intent(grpc_port).await?;
    client
        .approve_session_document(EmptyPresenterIntent {})
        .await?;
    Ok(())
}

pub async fn refine_session_document_localhost(grpc_port: u16) -> anyhow::Result<()> {
    let mut client = connect_presenter_intent(grpc_port).await?;
    client
        .refine_session_document(EmptyPresenterIntent {})
        .await?;
    Ok(())
}

pub async fn view_session_document_localhost(grpc_port: u16) -> anyhow::Result<()> {
    let mut client = connect_presenter_intent(grpc_port).await?;
    client
        .view_session_document(EmptyPresenterIntent {})
        .await?;
    Ok(())
}

pub async fn dismiss_viewer_localhost(grpc_port: u16) -> anyhow::Result<()> {
    let mut client = connect_presenter_intent(grpc_port).await?;
    client.dismiss_viewer(EmptyPresenterIntent {}).await?;
    Ok(())
}

pub async fn reject_session_document_localhost(grpc_port: u16) -> anyhow::Result<()> {
    let mut client = connect_presenter_intent(grpc_port).await?;
    client
        .reject_session_document(EmptyPresenterIntent {})
        .await?;
    Ok(())
}

pub async fn answer_clarification_select_localhost(
    grpc_port: u16,
    option_index: u32,
    clarification_question_index: Option<u32>,
) -> anyhow::Result<()> {
    let mut client = connect_presenter_intent(grpc_port).await?;
    client
        .answer_clarification_select(AnswerClarificationSelectRequest {
            option_index,
            clarification_question_index,
        })
        .await?;
    Ok(())
}

pub async fn answer_clarification_multi_select_localhost(
    grpc_port: u16,
    indices: Vec<u32>,
    other: String,
) -> anyhow::Result<()> {
    let mut client = connect_presenter_intent(grpc_port).await?;
    client
        .answer_clarification_multi_select(AnswerClarificationMultiSelectRequest { indices, other })
        .await?;
    Ok(())
}

pub async fn answer_clarification_text_localhost(grpc_port: u16, text: &str) -> anyhow::Result<()> {
    let mut client = connect_presenter_intent(grpc_port).await?;
    client
        .answer_clarification_text(AnswerClarificationTextRequest {
            text: text.to_string(),
        })
        .await?;
    Ok(())
}

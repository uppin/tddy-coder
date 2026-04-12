//! Unary gRPC: submit feature text into the presenter intent channel (LiveKit daemon).

use std::sync::mpsc::Sender;

use tonic::{Request, Response, Status};

use tddy_core::UserIntent;

use crate::gen::presenter_intent_server::PresenterIntent;
use crate::gen::{
    AnswerClarificationMultiSelectRequest, AnswerClarificationSelectRequest,
    AnswerClarificationTextRequest, EmptyPresenterIntent, SubmitFeatureTextRequest,
    SubmitFeatureTextResponse,
};

/// Forwards [`SubmitFeatureText`] to the presenter [`UserIntent`] channel.
pub struct PresenterIntentService {
    intent_tx: Sender<UserIntent>,
}

impl PresenterIntentService {
    pub fn new(intent_tx: Sender<UserIntent>) -> Self {
        Self { intent_tx }
    }
}

#[tonic::async_trait]
impl PresenterIntent for PresenterIntentService {
    async fn submit_feature_text(
        &self,
        request: Request<SubmitFeatureTextRequest>,
    ) -> Result<Response<SubmitFeatureTextResponse>, Status> {
        let text = request.into_inner().text;
        let t = text.trim().to_string();
        if t.is_empty() {
            return Err(Status::invalid_argument("empty feature text"));
        }
        let tx = self.intent_tx.clone();
        tokio::task::spawn_blocking(move || tx.send(UserIntent::SubmitFeatureInput(t)))
            .await
            .map_err(|e| Status::internal(format!("submit join: {e}")))?
            .map_err(|_| {
                Status::failed_precondition(
                    "presenter is not accepting feature input (session ended or not awaiting input)",
                )
            })?;
        Ok(Response::new(SubmitFeatureTextResponse {}))
    }

    async fn approve_session_document(
        &self,
        _request: Request<EmptyPresenterIntent>,
    ) -> Result<Response<SubmitFeatureTextResponse>, Status> {
        send_empty_intent(&self.intent_tx, UserIntent::ApproveSessionDocument).await
    }

    async fn refine_session_document(
        &self,
        _request: Request<EmptyPresenterIntent>,
    ) -> Result<Response<SubmitFeatureTextResponse>, Status> {
        send_empty_intent(&self.intent_tx, UserIntent::RefineSessionDocument).await
    }

    async fn view_session_document(
        &self,
        _request: Request<EmptyPresenterIntent>,
    ) -> Result<Response<SubmitFeatureTextResponse>, Status> {
        send_empty_intent(&self.intent_tx, UserIntent::ViewSessionDocument).await
    }

    async fn dismiss_viewer(
        &self,
        _request: Request<EmptyPresenterIntent>,
    ) -> Result<Response<SubmitFeatureTextResponse>, Status> {
        send_empty_intent(&self.intent_tx, UserIntent::DismissViewer).await
    }

    async fn reject_session_document(
        &self,
        _request: Request<EmptyPresenterIntent>,
    ) -> Result<Response<SubmitFeatureTextResponse>, Status> {
        send_empty_intent(&self.intent_tx, UserIntent::RejectSessionDocument).await
    }

    async fn answer_clarification_select(
        &self,
        request: Request<AnswerClarificationSelectRequest>,
    ) -> Result<Response<SubmitFeatureTextResponse>, Status> {
        let inner = request.into_inner();
        let option_index = inner.option_index as usize;
        let clarification_question_index = inner
            .clarification_question_index
            .map(|q| q as usize);
        send_empty_intent(
            &self.intent_tx,
            UserIntent::AnswerSelect {
                option_index,
                clarification_question_index,
            },
        )
        .await
    }

    async fn answer_clarification_multi_select(
        &self,
        request: Request<AnswerClarificationMultiSelectRequest>,
    ) -> Result<Response<SubmitFeatureTextResponse>, Status> {
        let inner = request.into_inner();
        let indices: Vec<usize> = inner.indices.into_iter().map(|i| i as usize).collect();
        let other = if inner.other.is_empty() {
            None
        } else {
            Some(inner.other)
        };
        send_empty_intent(
            &self.intent_tx,
            UserIntent::AnswerMultiSelect(indices, other),
        )
        .await
    }

    async fn answer_clarification_text(
        &self,
        request: Request<AnswerClarificationTextRequest>,
    ) -> Result<Response<SubmitFeatureTextResponse>, Status> {
        let text = request.into_inner().text;
        let t = text.trim().to_string();
        if t.is_empty() {
            return Err(Status::invalid_argument("empty clarification text"));
        }
        let tx = self.intent_tx.clone();
        tokio::task::spawn_blocking(move || tx.send(UserIntent::AnswerText(t)))
            .await
            .map_err(|e| Status::internal(format!("intent join: {e}")))?
            .map_err(|_| {
                Status::failed_precondition(
                    "presenter is not accepting text input (session ended or wrong mode)",
                )
            })?;
        Ok(Response::new(SubmitFeatureTextResponse {}))
    }
}

async fn send_empty_intent(
    tx: &Sender<UserIntent>,
    intent: UserIntent,
) -> Result<Response<SubmitFeatureTextResponse>, Status> {
    let tx = tx.clone();
    tokio::task::spawn_blocking(move || tx.send(intent))
        .await
        .map_err(|e| Status::internal(format!("intent join: {e}")))?
        .map_err(|_| {
            Status::failed_precondition(
                "presenter is not accepting this action (session ended or wrong mode)",
            )
        })?;
    Ok(Response::new(SubmitFeatureTextResponse {}))
}

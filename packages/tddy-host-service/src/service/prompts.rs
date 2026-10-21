use super::HostServiceImpl;

use tddy_service::proto::host::{AnswerHostPromptResponse, StreamHostPromptsRequest};

use tddy_service::proto::host::AnswerHostPromptRequest;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;

use std::sync::Arc;

use tddy_service::proto::host::HostPromptEvent;

use tddy_rpc::Status;

use crate::stream::MpscHostPromptStream;

use tddy_rpc::Response;

use tddy_rpc::Request;

impl HostServiceImpl {
    /// Body of [`HostService::stream_host_prompts`]; the trait method delegates here.
    pub(crate) async fn stream_host_prompts_impl(
        &self,
        request: Request<StreamHostPromptsRequest>,
    ) -> Result<Response<MpscHostPromptStream>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Routed before anything else, as `GetHostTooling` routes: a prompt is raised by, and
        // answerable on, exactly one host, and a browser that asked one host what it is waiting on
        // would take this daemon's own questions for that host's.
        if let Some(rx) = self
            .stream_served_by_peer("StreamHostPrompts", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(MpscHostPromptStream { rx }));
        }

        // Kept, not discarded: this is the identity the feed is filtered by. A prompt names a
        // private-key path one operator typed, and it is theirs alone to see and to answer.
        let subscriber = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Result<HostPromptEvent, Status>>();
        let prompts = Arc::clone(&self.host_prompts);
        let keypair = Arc::clone(&self.host_keypair);
        let daemon_instance_id = local_instance_id_for_config(&self.config);
        let counted = crate::host_prompt_stream::PumpCount::running(Arc::clone(&self.prompt_pumps));

        tokio::spawn(async move {
            // Moved into the task rather than dropped at the end of it, so the count falls when the
            // pump actually stops — including if it panics.
            let _counted = counted;
            crate::host_prompt_stream::pump_host_prompts(
                prompts,
                keypair,
                daemon_instance_id,
                subscriber,
                tx,
            )
            .await;
        });

        Ok(Response::new(MpscHostPromptStream { rx }))
    }

    /// Body of [`HostService::answer_host_prompt`]; the trait method delegates here.
    pub(crate) async fn answer_host_prompt_impl(
        &self,
        request: Request<AnswerHostPromptRequest>,
    ) -> Result<Response<AnswerHostPromptResponse>, Status> {
        self.record_rpc_activity();
        let req = request.into_inner();

        // Routed before the prompt is looked up, and before the caller is authenticated — the same
        // order `GetHostTooling` uses, and for the same reason: the prompt this answers exists on
        // the host that raised it, and resolved here it belongs to nothing. Answered locally, an
        // operator's passphrase would be spent on a refusal from the wrong machine.
        if let Some(answered) = self
            .rpc_served_by_peer("AnswerHostPrompt", &req.daemon_instance_id, &req)
            .await?
        {
            return Ok(Response::new(answered));
        }

        let answered_by = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;

        // A refusal is a `false` on the response, not a `Status` error: an expired or replayed
        // prompt is an ordinary outcome of an operator taking their time, and the browser has to
        // tell the operator which of the three it was.
        //
        // Accepting the ciphertext is what hands it over: the registry passes it straight down the
        // handoff `AddHostKey` is waiting on, which decrypts it, unlocks the key, adds the identity
        // and drops the plaintext. An answer nobody is waiting for is still recorded as this
        // prompt's one answer, and its ciphertext is dropped rather than kept.
        //
        // Nothing about the payload is logged here at any level, deliberately: a passphrase must
        // never reach a log, and the cheapest way to keep that true is for this handler to have
        // nothing to say about what it was given.
        //
        // The answering session's own identity decides which prompt it may answer: a prompt raised
        // by somebody else is refused exactly as an id that was never issued is, and is left
        // unspent for the operator it belongs to.
        let answered = self.host_prompts.answer(
            &req.prompt_id,
            &answered_by,
            req.encrypted_answer,
            crate::host_registry::now_unix_ms(),
        );
        Ok(Response::new(match answered {
            Ok(()) => AnswerHostPromptResponse {
                accepted: true,
                rejection_reason: String::new(),
            },
            Err(rejection) => AnswerHostPromptResponse {
                accepted: false,
                rejection_reason: crate::host_messages::rejection_reason(&rejection),
            },
        }))
    }
}

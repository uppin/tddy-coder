use crate::connection_service::DaemonSessionHost;
use async_trait::async_trait;
use tddy_rpc::Request;
use tddy_service::proto::activity::{
    ActivityService, AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse,
    ReportAgentActivityRequest, ReportSessionStatusRequest, ReportSessionStatusResponse,
};

/// A target session's followed hooks, delivered to this daemon's activity handling: each report
/// is served by the same `activity_service()` a local hook's RPC reaches.
pub struct ActivityHookDelivery {
    pub host: DaemonSessionHost,
}

#[async_trait]
impl tddy_agent_launch::remote_hook_follower::HookReportDelivery for ActivityHookDelivery {
    async fn report_session_status(
        &self,
        request: ReportSessionStatusRequest,
    ) -> Result<
        ReportSessionStatusResponse,
        tddy_agent_launch::remote_hook_follower::HookDeliveryError,
    > {
        use tddy_agent_launch::remote_hook_follower::HookDeliveryError;
        self.host
            .activity_service()
            .report_session_status(Request::direct(request))
            .await
            .map(|response| response.into_inner())
            .map_err(HookDeliveryError::from_status)
    }

    async fn await_question_answer(
        &self,
        request: AwaitQuestionAnswerRequest,
    ) -> Result<
        AwaitQuestionAnswerResponse,
        tddy_agent_launch::remote_hook_follower::HookDeliveryError,
    > {
        use tddy_agent_launch::remote_hook_follower::HookDeliveryError;
        self.host
            .activity_service()
            .await_question_answer(Request::direct(request))
            .await
            .map(|response| response.into_inner())
            .map_err(HookDeliveryError::from_status)
    }

    async fn report_agent_activity(
        &self,
        request: ReportAgentActivityRequest,
    ) -> Result<(), tddy_agent_launch::remote_hook_follower::HookDeliveryError> {
        use tddy_agent_launch::remote_hook_follower::HookDeliveryError;
        self.host
            .activity_service()
            .report_agent_activity(Request::direct(request))
            .await
            .map(|_| ())
            .map_err(HookDeliveryError::from_status)
    }

    async fn record_transcript_line(
        &self,
        line: tddy_session_activity::TranscriptLine,
    ) -> Result<(), tddy_agent_launch::remote_hook_follower::HookDeliveryError> {
        use tddy_agent_launch::remote_hook_follower::HookDeliveryError;
        self.host
            .activity_service()
            .record_transcript_line(line)
            .await
            .map_err(HookDeliveryError::from_status)
    }

    async fn post_message_to_orchestrator(
        &self,
        message: tddy_agent_launch::back_channel::RemoteMessage,
    ) -> Result<(), tddy_agent_launch::remote_hook_follower::HookDeliveryError> {
        use tddy_agent_launch::remote_hook_follower::HookDeliveryError;
        self.host
            .post_remote_message_to_orchestrator(message)
            .await
            .map_err(HookDeliveryError::failed_precondition)
    }
}

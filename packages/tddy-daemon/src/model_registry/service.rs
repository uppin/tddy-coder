//! `ModelRegistryServiceImpl` — the daemon-side implementation of `models.ModelRegistryService`.
//!
//! Every RPC is scoped to this daemon: its own SQLite registry, its own provider endpoints. There
//! is no cross-daemon forwarding here; the web fans out to each common-room daemon and merges.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_discovery::agent_def::SubagentTool;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::models::{
    AssignableTool, CreateAssistantRequest, CreateAssistantResponse, CreateProviderRequest,
    CreateProviderResponse, DeleteAssistantRequest, DeleteAssistantResponse, DeleteProviderRequest,
    DeleteProviderResponse, ListAssignableToolsRequest, ListAssignableToolsResponse,
    ListAssistantsRequest, ListAssistantsResponse, ListModelsRequest, ListModelsResponse,
    ListProvidersRequest, ListProvidersResponse, LoadModelRequest, LoadModelResponse, ModelEntry,
    ModelRegistryService, ProviderEntry, ProviderKind, RefreshProviderModelsRequest,
    RefreshProviderModelsResponse, UnloadModelRequest, UnloadModelResponse,
};

use super::error::ModelRegistryError;
use super::labels::UNDETERMINABLE_LABEL;
use super::provider_client::{ProviderClient, ProviderClientFactory};
use super::store::{ModelRegistryStore, NewAssistant, NewProvider};
use crate::task_service::SessionUserResolver;

pub struct ModelRegistryServiceImpl {
    store: Arc<ModelRegistryStore>,
    clients: Arc<dyn ProviderClientFactory>,
    user_resolver: SessionUserResolver,
}

impl ModelRegistryServiceImpl {
    pub fn new(
        store: Arc<ModelRegistryStore>,
        clients: Arc<dyn ProviderClientFactory>,
        user_resolver: SessionUserResolver,
    ) -> Self {
        Self {
            store,
            clients,
            user_resolver,
        }
    }

    fn authenticate(&self, token: &str) -> Result<String, Status> {
        (self.user_resolver)(token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session token"))
    }

    /// The live client for `provider_id`, carrying that provider's stored credential.
    ///
    /// `caller` is the operator the session token resolved to. Reading the credential is the
    /// owner's alone, so a refresh, load or unload aimed at a colleague's provider is refused here
    /// rather than being run against their endpoint without their key.
    async fn client_for(
        &self,
        provider_id: &str,
        caller: &str,
    ) -> Result<(ProviderEntry, Arc<dyn ProviderClient>), ModelRegistryError> {
        let provider = self.store.provider(provider_id).await?;
        let credential = self.store.credential_for(provider_id, caller).await?;
        let client = self.clients.client_for(&provider, credential)?;
        Ok((provider, client))
    }

    /// Run `operation` (load or unload) and report the model as it stands afterwards.
    ///
    /// The residency check comes from the provider itself rather than the cached row, and the
    /// cache is corrected to match. A model the cache has never seen is still operated on: the
    /// caller addressed it by provider and id, which is all the provider needs.
    async fn apply_residency<'a, F, Fut>(
        &'a self,
        provider_id: &'a str,
        model_id: &'a str,
        caller: &'a str,
        operation: F,
    ) -> Result<ModelEntry, Status>
    where
        F: FnOnce(Arc<dyn ProviderClient>) -> Fut,
        Fut: std::future::Future<Output = Result<(), ModelRegistryError>>,
    {
        let (provider, client) = self.client_for(provider_id, caller).await?;
        operation(Arc::clone(&client)).await?;

        let load_state = client.load_state(model_id).await?;
        self.store
            .set_load_state(provider_id, model_id, load_state as i32)
            .await?;

        match self.store.model(provider_id, model_id).await? {
            Some(model) => Ok(model),
            // Not in the cached catalog (no refresh since it appeared on the host). Report the
            // facts the operation established rather than inventing catalog metadata for it —
            // including its labels, which are `unknown` rather than empty: `models.proto` gives
            // "we could not tell" its own value, and an empty list would read as "this model has
            // no capabilities at all".
            None => Ok(ModelEntry {
                model_id: model_id.to_string(),
                provider_id: provider_id.to_string(),
                label: model_id.to_string(),
                labels: vec![UNDETERMINABLE_LABEL.to_string()],
                load_state: load_state as i32,
                daemon_instance_id: provider.daemon_instance_id,
                size_bytes: 0,
            }),
        }
    }
}

#[async_trait]
impl ModelRegistryService for ModelRegistryServiceImpl {
    async fn list_providers(
        &self,
        request: Request<ListProvidersRequest>,
    ) -> Result<Response<ListProvidersResponse>, Status> {
        self.authenticate(&request.get_ref().session_token)?;
        let providers = self.store.list_providers().await?;
        Ok(Response::new(ListProvidersResponse { providers }))
    }

    async fn create_provider(
        &self,
        request: Request<CreateProviderRequest>,
    ) -> Result<Response<CreateProviderResponse>, Status> {
        let req = request.into_inner();
        let caller = self.authenticate(&req.session_token)?;
        let kind = ProviderKind::try_from(req.kind)
            .map_err(|_| Status::invalid_argument(format!("unknown provider kind {}", req.kind)))?;
        if kind == ProviderKind::Unspecified {
            return Err(Status::invalid_argument("a provider kind is required"));
        }
        let provider = self
            .store
            .create_provider(
                NewProvider {
                    kind,
                    label: req.label,
                    base_url: req.base_url,
                    api_key: (!req.api_key.is_empty()).then_some(req.api_key),
                },
                &caller,
            )
            .await?;
        Ok(Response::new(CreateProviderResponse {
            provider: Some(provider),
        }))
    }

    async fn delete_provider(
        &self,
        request: Request<DeleteProviderRequest>,
    ) -> Result<Response<DeleteProviderResponse>, Status> {
        let req = request.into_inner();
        let caller = self.authenticate(&req.session_token)?;
        self.store
            .delete_provider(&req.provider_id, &caller)
            .await?;
        Ok(Response::new(DeleteProviderResponse {}))
    }

    async fn list_models(
        &self,
        request: Request<ListModelsRequest>,
    ) -> Result<Response<ListModelsResponse>, Status> {
        self.authenticate(&request.get_ref().session_token)?;
        let models = self.store.list_models().await?;
        Ok(Response::new(ListModelsResponse { models }))
    }

    /// Re-enumerate one provider and replace its cached catalog.
    ///
    /// A failed enumeration is reported as a failure: the cache is left untouched and the reason
    /// is both returned to the caller and recorded on the provider row, so the screen can explain
    /// the stale catalog instead of presenting it as current.
    async fn refresh_provider_models(
        &self,
        request: Request<RefreshProviderModelsRequest>,
    ) -> Result<Response<RefreshProviderModelsResponse>, Status> {
        let req = request.into_inner();
        let caller = self.authenticate(&req.session_token)?;
        let (_provider, client) = self.client_for(&req.provider_id, &caller).await?;

        let models = match client.list_models().await {
            Ok(models) => models,
            Err(e) => {
                // Recording why is a courtesy to the screen; it is not what the caller asked
                // about. Propagating a failure to record here would replace "your Ollama refused
                // the connection" with a storage error, hiding the only fact worth knowing — so a
                // recording failure is logged and the provider's own error is still what returns.
                if let Err(recording) = self
                    .store
                    .record_enumeration_error(&req.provider_id, &e.to_string())
                    .await
                {
                    log::warn!(
                        target: "tddy_daemon::model_registry",
                        "could not record the failed enumeration of {}: {recording}",
                        req.provider_id
                    );
                }
                return Err(e.into());
            }
        };

        // Both writes in one transaction: separately, a crash between them leaves the fresh
        // catalog under the previous run's error message, which the screen renders as "these
        // models are stale" about models that are not.
        self.store.record_refresh(&req.provider_id, &models).await?;
        Ok(Response::new(RefreshProviderModelsResponse { models }))
    }

    async fn load_model(
        &self,
        request: Request<LoadModelRequest>,
    ) -> Result<Response<LoadModelResponse>, Status> {
        let req = request.into_inner();
        let caller = self.authenticate(&req.session_token)?;
        let model_id = req.model_id.clone();
        let model = self
            .apply_residency(
                &req.provider_id,
                &req.model_id,
                &caller,
                move |client| async move { client.load(&model_id).await },
            )
            .await?;
        Ok(Response::new(LoadModelResponse { model: Some(model) }))
    }

    async fn unload_model(
        &self,
        request: Request<UnloadModelRequest>,
    ) -> Result<Response<UnloadModelResponse>, Status> {
        let req = request.into_inner();
        let caller = self.authenticate(&req.session_token)?;
        let model_id = req.model_id.clone();
        let model = self
            .apply_residency(
                &req.provider_id,
                &req.model_id,
                &caller,
                move |client| async move { client.unload(&model_id).await },
            )
            .await?;
        Ok(Response::new(UnloadModelResponse { model: Some(model) }))
    }

    async fn list_assistants(
        &self,
        request: Request<ListAssistantsRequest>,
    ) -> Result<Response<ListAssistantsResponse>, Status> {
        self.authenticate(&request.get_ref().session_token)?;
        let assistants = self.store.list_assistants().await?;
        Ok(Response::new(ListAssistantsResponse { assistants }))
    }

    async fn create_assistant(
        &self,
        request: Request<CreateAssistantRequest>,
    ) -> Result<Response<CreateAssistantResponse>, Status> {
        let req = request.into_inner();
        let caller = self.authenticate(&req.session_token)?;
        let assistant = self
            .store
            .create_assistant(
                NewAssistant {
                    name: req.name,
                    label: req.label,
                    provider_id: req.provider_id,
                    model_id: req.model_id,
                    system_prompt: req.system_prompt,
                    tools: req.tools,
                    replaces: req.replaces,
                },
                &caller,
            )
            .await?;
        Ok(Response::new(CreateAssistantResponse {
            assistant: Some(assistant),
        }))
    }

    async fn update_assistant(
        &self,
        request: Request<tddy_service::proto::models::UpdateAssistantRequest>,
    ) -> Result<Response<tddy_service::proto::models::UpdateAssistantResponse>, Status> {
        let req = request.into_inner();
        let caller = self.authenticate(&req.session_token)?;
        let assistant = self
            .store
            .update_assistant(
                &req.assistant_id,
                &req.label,
                &req.system_prompt,
                &req.tools,
                &req.replaces,
                &caller,
            )
            .await?;
        Ok(Response::new(
            tddy_service::proto::models::UpdateAssistantResponse {
                assistant: Some(assistant),
            },
        ))
    }

    async fn delete_assistant(
        &self,
        request: Request<DeleteAssistantRequest>,
    ) -> Result<Response<DeleteAssistantResponse>, Status> {
        let req = request.into_inner();
        let caller = self.authenticate(&req.session_token)?;
        self.store
            .delete_assistant(&req.assistant_id, &caller)
            .await?;
        Ok(Response::new(DeleteAssistantResponse {}))
    }

    /// The exec catalog, verbatim — the tools an assistant may be given are exactly the tools this
    /// daemon can dispatch, so the web renders no list of its own.
    async fn list_assignable_tools(
        &self,
        request: Request<ListAssignableToolsRequest>,
    ) -> Result<Response<ListAssignableToolsResponse>, Status> {
        self.authenticate(&request.get_ref().session_token)?;
        let tools = tddy_tool_engine::catalog::tool_catalog()
            .into_iter()
            .map(|tool| {
                // An engine tool with no `SubagentTool` variant could not be bound to an assistant
                // at all; reporting it as non-mutating would be a guess about a tool we cannot
                // classify.
                let subagent_tool =
                    SubagentTool::from_catalog_name(&tool.name).ok_or_else(|| {
                        Status::internal(format!(
                            "exec-catalog tool '{}' has no SubagentTool variant",
                            tool.name
                        ))
                    })?;
                Ok(AssignableTool {
                    name: tool.name,
                    description: tool.description,
                    is_mutating: subagent_tool.is_mutating(),
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;
        Ok(Response::new(ListAssignableToolsResponse { tools }))
    }
}

/// The production factory: every provider kind resolved to the client that speaks its API, and
/// nothing else.
///
/// Each kind is matched by name. There is deliberately no catch-all arm: a row whose `kind` is
/// `PROVIDER_KIND_UNSPECIFIED`, or an integer this build has no variant for (a corrupted row, or a
/// newer daemon writing the same database), is a provider this process cannot classify — and
/// falling through to "probably OpenAI-compatible" would send that provider's stored api key to an
/// endpoint nobody decided it belonged to.
pub struct DefaultProviderClients;

impl ProviderClientFactory for DefaultProviderClients {
    fn client_for(
        &self,
        provider: &ProviderEntry,
        credential: Option<String>,
    ) -> Result<Arc<dyn ProviderClient>, ModelRegistryError> {
        let kind = ProviderKind::try_from(provider.kind).map_err(|_| {
            ModelRegistryError::UnsupportedOperation(format!(
                "provider {} has kind {}, which this daemon has no client for",
                provider.provider_id, provider.kind
            ))
        })?;
        match kind {
            ProviderKind::Ollama => Ok(Arc::new(super::ollama::OllamaProviderClient::new(
                &provider.base_url,
                &provider.provider_id,
                &provider.daemon_instance_id,
                credential,
            ))),
            ProviderKind::Openai | ProviderKind::Fireworks => Ok(Arc::new(
                super::openai_compatible::OpenAiCompatibleProviderClient::new(
                    &provider.base_url,
                    &provider.provider_id,
                    &provider.daemon_instance_id,
                    credential,
                ),
            )),
            // Anthropic authenticates with `x-api-key` and requires `anthropic-version`; a bearer
            // token is refused with a 401 whose body would then be the provider's whole
            // enumeration error.
            ProviderKind::Anthropic => Ok(Arc::new(
                super::openai_compatible::OpenAiCompatibleProviderClient::with_credential_style(
                    &provider.base_url,
                    &provider.provider_id,
                    &provider.daemon_instance_id,
                    credential,
                    super::openai_compatible::CredentialStyle::AnthropicApiKey,
                ),
            )),
            ProviderKind::Unspecified => Err(ModelRegistryError::UnsupportedOperation(format!(
                "provider {} has no kind, so this daemon cannot tell which api it speaks",
                provider.provider_id
            ))),
        }
    }
}

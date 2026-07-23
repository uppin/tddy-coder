//! The `bsp.BspService` RPC implementation.
//!
//! Read methods project the per-session catalog's `build_targets` table (see
//! [`tddy_core::session_catalog`]); build ops delegate to [`tddy_build::service::build_json`] and are
//! capability-gated. Served over the workspace's protobuf/Connect + LiveKit transports.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tddy_build::capabilities::BuildMode;
use tddy_core::session_catalog::{BuildTargetSummary, SessionCatalog};
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::bsp::{
    ActionOutcome, BspService as BspServiceTrait, BuildTarget, BuildTargetActionRequest,
    BuildTargetActionResponse, BuildTargetCapabilities, BuildTargetIdentifier,
    BuildTargetOutputPathsRequest, BuildTargetOutputPathsResponse, BuildTargetResult,
    BuildTargetSourcesRequest, BuildTargetSourcesResponse, OutputPathItem, OutputPathsItem,
    SourceItem, SourcesItem, WorkspaceBuildTargetsRequest, WorkspaceBuildTargetsResponse,
    WorkspaceReloadRequest, WorkspaceReloadResponse,
};
use tddy_task::TaskRegistry;
use tokio::sync::Mutex;

use crate::plugins::plugin_registry;

/// Map a catalog [`BuildTargetSummary`] onto the BSP `BuildTarget` wire message.
pub fn to_bsp_target(summary: &BuildTargetSummary) -> BuildTarget {
    BuildTarget {
        id: summary.id.clone(),
        display_name: summary.name.clone(),
        base_directory: summary.base_dir.clone().unwrap_or_default(),
        tags: summary.tags.clone(),
        language_ids: summary.languages.clone(),
        dependencies: summary
            .deps
            .iter()
            .map(|d| BuildTargetIdentifier { id: d.clone() })
            .collect(),
        capabilities: Some(BuildTargetCapabilities {
            can_compile: summary.capabilities.compile,
            can_test: summary.capabilities.test,
            can_run: summary.capabilities.run,
            can_debug: summary.capabilities.debug,
        }),
    }
}

/// The BSP service backed by a session's catalog + the repo's build graph.
pub struct BspServiceImpl {
    /// The session directory that owns `catalog.db`.
    pub session_dir: PathBuf,
    /// The repository root the build targets are discovered from.
    pub repo_root: PathBuf,
    /// The tddy data dir (action-manifest store root) the populate scan reads.
    pub tddy_data_dir: PathBuf,
    /// The task registry the populate scan is spawned on.
    task_registry: TaskRegistry,
    /// The opened session catalog, populated lazily on first read and replaced on reload.
    catalog: Mutex<Option<Arc<SessionCatalog>>>,
}

impl BspServiceImpl {
    pub fn new(session_dir: PathBuf, repo_root: PathBuf, tddy_data_dir: PathBuf) -> Self {
        Self {
            session_dir,
            repo_root,
            tddy_data_dir,
            task_registry: TaskRegistry::new(),
            catalog: Mutex::new(None),
        }
    }

    /// A stable session id derived from the session directory.
    fn session_id(&self) -> String {
        self.session_dir.to_string_lossy().into_owned()
    }

    /// Open + populate a fresh catalog handle (re-scans `BUILD.yaml`).
    async fn open_catalog(&self) -> Result<Arc<SessionCatalog>, Status> {
        SessionCatalog::open_and_populate(
            &self.session_dir,
            Some(&self.repo_root),
            &self.tddy_data_dir,
            &self.task_registry,
            &self.session_id(),
            tddy_core::session_catalog::build_catalog_provider(),
        )
        .await
        .map_err(|e| Status::internal(e.to_string()))
    }

    /// The cached catalog handle, opening + populating it on first use.
    async fn catalog(&self) -> Result<Arc<SessionCatalog>, Status> {
        let mut guard = self.catalog.lock().await;
        if let Some(existing) = guard.as_ref() {
            return Ok(existing.clone());
        }
        let catalog = self.open_catalog().await?;
        *guard = Some(catalog.clone());
        Ok(catalog)
    }

    /// Force a fresh scan, replacing the cached handle (used by `workspace_reload`).
    async fn reload_catalog(&self) -> Result<Arc<SessionCatalog>, Status> {
        let catalog = self.open_catalog().await?;
        *self.catalog.lock().await = Some(catalog.clone());
        Ok(catalog)
    }

    /// Look up the summaries of all build targets in the session.
    async fn summaries(&self) -> Result<Vec<BuildTargetSummary>, Status> {
        let catalog = self.catalog().await?;
        catalog
            .list_build_targets(&Default::default())
            .await
            .map_err(|e| Status::internal(e.to_string()))
    }

    /// Execute `mode` for each requested target, capability-gating Test/Run.
    async fn run_action(
        &self,
        request: BuildTargetActionRequest,
        mode: BuildMode,
    ) -> Result<Response<BuildTargetActionResponse>, Status> {
        let summaries = self.summaries().await?;
        let registry = plugin_registry();
        let mut results = Vec::new();
        for target in &request.targets {
            results.push(
                self.execute_one(&summaries, &registry, &target.id, mode, &request)
                    .await,
            );
        }
        Ok(Response::new(BuildTargetActionResponse { results }))
    }

    async fn execute_one(
        &self,
        summaries: &[BuildTargetSummary],
        registry: &tddy_build::PluginRegistry,
        target_id: &str,
        mode: BuildMode,
        request: &BuildTargetActionRequest,
    ) -> BuildTargetResult {
        let identifier = BuildTargetIdentifier {
            id: target_id.to_string(),
        };

        // Capability-gate Test/Run before touching the build path.
        if mode != BuildMode::Compile {
            let supported = summaries
                .iter()
                .find(|s| s.id == target_id)
                .map(|s| match mode {
                    BuildMode::Test => s.capabilities.test,
                    BuildMode::Run => s.capabilities.run,
                    BuildMode::Compile => true,
                })
                .unwrap_or(false);
            if !supported {
                return BuildTargetResult {
                    target: Some(identifier),
                    status: "error".to_string(),
                    actions: Vec::new(),
                    error_message: format!("target {target_id} does not support {}", mode.label()),
                };
            }
        }

        // Test/Run must actually execute every invocation — the build cache is keyed on inputs and
        // would return a stale "success" for unchanged sources, silently skipping the run. Only
        // Compile (a pure artifact producer) is safely cacheable.
        let no_cache = request.no_cache || mode != BuildMode::Compile;
        match tddy_build::service::build_json(
            &self.repo_root,
            target_id,
            no_cache,
            request.dry_run,
            mode,
            registry,
        )
        .await
        {
            Ok(value) => BuildTargetResult {
                target: Some(identifier),
                status: "ok".to_string(),
                actions: parse_actions(&value),
                error_message: String::new(),
            },
            Err(e) => BuildTargetResult {
                target: Some(identifier),
                status: "error".to_string(),
                actions: Vec::new(),
                error_message: e.to_string(),
            },
        }
    }
}

/// Parse the `actions` array of a `build_json` record into wire [`ActionOutcome`]s.
fn parse_actions(value: &serde_json::Value) -> Vec<ActionOutcome> {
    let Some(actions) = value.get("actions").and_then(|a| a.as_array()) else {
        return Vec::new();
    };
    actions
        .iter()
        .map(|a| ActionOutcome {
            action_id: a
                .get("action_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            cached: a.get("cached").and_then(|v| v.as_bool()).unwrap_or(false),
            exit_code: a.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            argv: a
                .get("argv")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            stdout: a
                .get("stdout")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            stderr: a
                .get("stderr")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        })
        .collect()
}

#[async_trait]
impl BspServiceTrait for BspServiceImpl {
    async fn workspace_build_targets(
        &self,
        _request: Request<WorkspaceBuildTargetsRequest>,
    ) -> Result<Response<WorkspaceBuildTargetsResponse>, Status> {
        let targets = self.summaries().await?.iter().map(to_bsp_target).collect();
        Ok(Response::new(WorkspaceBuildTargetsResponse { targets }))
    }

    async fn workspace_reload(
        &self,
        _request: Request<WorkspaceReloadRequest>,
    ) -> Result<Response<WorkspaceReloadResponse>, Status> {
        self.reload_catalog().await?;
        Ok(Response::new(WorkspaceReloadResponse {}))
    }

    async fn build_target_sources(
        &self,
        request: Request<BuildTargetSourcesRequest>,
    ) -> Result<Response<BuildTargetSourcesResponse>, Status> {
        let requested = request.into_inner().targets;
        let summaries = self.summaries().await?;
        let items = requested
            .into_iter()
            .map(|id| {
                let sources = summaries
                    .iter()
                    .find(|s| s.id == id.id)
                    .map(|s| s.sources.clone())
                    .unwrap_or_default();
                SourcesItem {
                    target: Some(id),
                    sources: sources.into_iter().map(|uri| SourceItem { uri }).collect(),
                }
            })
            .collect();
        Ok(Response::new(BuildTargetSourcesResponse { items }))
    }

    async fn build_target_output_paths(
        &self,
        request: Request<BuildTargetOutputPathsRequest>,
    ) -> Result<Response<BuildTargetOutputPathsResponse>, Status> {
        let requested = request.into_inner().targets;
        let summaries = self.summaries().await?;
        let items = requested
            .into_iter()
            .map(|id| {
                let outputs = summaries
                    .iter()
                    .find(|s| s.id == id.id)
                    .map(|s| s.outputs.clone())
                    .unwrap_or_default();
                OutputPathsItem {
                    target: Some(id),
                    output_paths: outputs
                        .into_iter()
                        .map(|uri| OutputPathItem { uri })
                        .collect(),
                }
            })
            .collect();
        Ok(Response::new(BuildTargetOutputPathsResponse { items }))
    }

    async fn build_target_compile(
        &self,
        request: Request<BuildTargetActionRequest>,
    ) -> Result<Response<BuildTargetActionResponse>, Status> {
        self.run_action(request.into_inner(), BuildMode::Compile)
            .await
    }

    async fn build_target_test(
        &self,
        request: Request<BuildTargetActionRequest>,
    ) -> Result<Response<BuildTargetActionResponse>, Status> {
        self.run_action(request.into_inner(), BuildMode::Test).await
    }

    async fn build_target_run(
        &self,
        request: Request<BuildTargetActionRequest>,
    ) -> Result<Response<BuildTargetActionResponse>, Status> {
        self.run_action(request.into_inner(), BuildMode::Run).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_core::session_catalog::CatalogCapabilities;

    fn a_summary() -> BuildTargetSummary {
        BuildTargetSummary {
            id: "packages/foo:lib".to_string(),
            name: "Foo library".to_string(),
            package: "packages/foo".to_string(),
            base_dir: Some("packages/foo".to_string()),
            target_type: Some("rust_library".to_string()),
            tags: vec!["library".to_string()],
            languages: vec!["rust".to_string()],
            deps: vec!["packages/core:lib".to_string()],
            sources: vec!["packages/foo/src/lib.rs".to_string()],
            outputs: vec![],
            capabilities: CatalogCapabilities {
                compile: true,
                test: true,
                run: false,
                debug: false,
            },
            source_path: "/repo/packages/foo/BUILD.yaml".to_string(),
        }
    }

    #[test]
    fn maps_a_catalog_summary_onto_the_bsp_build_target() {
        // Given
        let summary = a_summary();

        // When
        let target = to_bsp_target(&summary);

        // Then
        assert_eq!(target.id, "packages/foo:lib");
        assert_eq!(target.display_name, "Foo library");
        assert_eq!(target.base_directory, "packages/foo");
        assert_eq!(target.tags, vec!["library".to_string()]);
        assert_eq!(target.language_ids, vec!["rust".to_string()]);
        let deps: Vec<&str> = target.dependencies.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(deps, vec!["packages/core:lib"]);
        let caps = target.capabilities.expect("capabilities present");
        assert!(caps.can_compile && caps.can_test);
        assert!(!caps.can_run && !caps.can_debug);
    }
}

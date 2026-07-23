//! Acceptance tests for the BSP-shaped build server service (`bsp.BspService`), served in-process
//! over a `tddy-rpc` bridge against a temp-repo `BUILD.yaml` — the same shape as
//! `tddy-service`'s `token_service_acceptance`.

use prost::Message;
use tempfile::TempDir;

use tddy_bsp::BspServiceImpl;
use tddy_rpc::{RequestMetadata, ResponseBody, RpcBridge, RpcMessage};
use tddy_service::proto::bsp::{
    BuildTarget, BuildTargetActionRequest, BuildTargetActionResponse, BuildTargetIdentifier,
    BuildTargetSourcesRequest, BuildTargetSourcesResponse, WorkspaceBuildTargetsRequest,
    WorkspaceBuildTargetsResponse, WorkspaceReloadRequest, WorkspaceReloadResponse,
};
use tddy_service::BspServiceServer;

const SERVICE: &str = "bsp.BspService";

/// A repo with three targets under `packages/foo`: a rust library (explicit capabilities), a rust
/// binary (capabilities derived), and a docker image (compile-only, derived).
const THREE_TARGETS: &str = "\
schema_version: 1
targets:
  - id: \"packages/foo:lib\"
    name: Foo library
    tags: [library]
    languages: [rust]
    capabilities: { compile: true, test: true, run: false, debug: false }
    config:
      type: rust_library
      package: foo
      srcs: [\"packages/foo/src/lib.rs\", \"packages/foo/Cargo.toml\"]
  - id: \"packages/foo:app\"
    name: Foo app
    config:
      type: rust_binary
      package: foo
      bin_name: foo
  - id: \"packages/foo:image\"
    name: Foo image
    config:
      type: docker_image
      dockerfile: packages/foo/Dockerfile
      context: .
      tag: foo:latest
";

/// In-process harness: a `BspServiceServer` behind an `RpcBridge`, over a temp repo + session dir.
struct BspHarness {
    bridge: RpcBridge<BspServiceServer<BspServiceImpl>>,
    repo: TempDir,
    _session: TempDir,
    _data: TempDir,
}

fn a_bsp_service_over(build_yaml: &str) -> BspHarness {
    tddy_bsp::register_catalog_provider();

    let repo = tempfile::tempdir().expect("repo tempdir");
    std::fs::create_dir_all(repo.path().join("packages/foo/src")).expect("mkdir packages/foo/src");
    std::fs::write(repo.path().join("packages/foo/BUILD.yaml"), build_yaml)
        .expect("write BUILD.yaml");

    let session = tempfile::tempdir().expect("session tempdir");
    let data = tempfile::tempdir().expect("data tempdir");

    let server = BspServiceServer::new(BspServiceImpl::new(
        session.path().to_path_buf(),
        repo.path().to_path_buf(),
        data.path().to_path_buf(),
    ));
    BspHarness {
        bridge: RpcBridge::new(server),
        repo,
        _session: session,
        _data: data,
    }
}

impl BspHarness {
    fn rewrite_build_yaml(&self, build_yaml: &str) {
        std::fs::write(self.repo.path().join("packages/foo/BUILD.yaml"), build_yaml)
            .expect("rewrite BUILD.yaml");
    }

    async fn call<Req: Message, Resp: Message + Default>(&self, method: &str, req: Req) -> Resp {
        let msg = RpcMessage {
            payload: req.encode_to_vec(),
            metadata: RequestMetadata::default(),
        };
        let body = self
            .bridge
            .handle_messages(SERVICE, method, &[msg])
            .await
            .expect("rpc call should succeed");
        let chunks = match body {
            ResponseBody::Complete(chunks) => chunks,
            _ => panic!("expected a unary Complete response"),
        };
        assert_eq!(chunks.len(), 1, "expected exactly one response chunk");
        Resp::decode(&chunks[0][..]).expect("decode response")
    }

    async fn workspace_build_targets(&self) -> Vec<BuildTarget> {
        let resp: WorkspaceBuildTargetsResponse = self
            .call(
                "WorkspaceBuildTargets",
                WorkspaceBuildTargetsRequest::default(),
            )
            .await;
        resp.targets
    }

    async fn reload(&self) {
        let _: WorkspaceReloadResponse = self
            .call("WorkspaceReload", WorkspaceReloadRequest::default())
            .await;
    }

    async fn sources(&self, target_id: &str) -> Vec<String> {
        let resp: BuildTargetSourcesResponse = self
            .call(
                "BuildTargetSources",
                BuildTargetSourcesRequest {
                    targets: vec![id(target_id)],
                    ..Default::default()
                },
            )
            .await;
        resp.items
            .into_iter()
            .flat_map(|item| item.sources)
            .map(|s| s.uri)
            .collect()
    }

    async fn compile(&self, target_id: &str, dry_run: bool) -> BuildTargetActionResponse {
        self.call(
            "BuildTargetCompile",
            BuildTargetActionRequest {
                targets: vec![id(target_id)],
                no_cache: false,
                dry_run,
                ..Default::default()
            },
        )
        .await
    }

    async fn test(&self, target_id: &str) -> BuildTargetActionResponse {
        self.call(
            "BuildTargetTest",
            BuildTargetActionRequest {
                targets: vec![id(target_id)],
                no_cache: false,
                dry_run: true,
                ..Default::default()
            },
        )
        .await
    }
}

fn id(target_id: &str) -> BuildTargetIdentifier {
    BuildTargetIdentifier {
        id: target_id.to_string(),
    }
}

fn find<'a>(targets: &'a [BuildTarget], target_id: &str) -> &'a BuildTarget {
    targets
        .iter()
        .find(|t| t.id == target_id)
        .unwrap_or_else(|| panic!("target {target_id} not found in {:?}", ids(targets)))
}

fn ids(targets: &[BuildTarget]) -> Vec<&str> {
    targets.iter().map(|t| t.id.as_str()).collect()
}

#[tokio::test]
async fn workspace_build_targets_lists_targets_with_capabilities_tags_and_languages() {
    // Given
    let bsp = a_bsp_service_over(THREE_TARGETS);

    // When
    let targets = bsp.workspace_build_targets().await;

    // Then — every BUILD.yaml target appears, with declared-or-derived metadata.
    let mut listed = ids(&targets);
    listed.sort_unstable();
    assert_eq!(
        listed,
        vec!["packages/foo:app", "packages/foo:image", "packages/foo:lib"]
    );

    let lib = find(&targets, "packages/foo:lib");
    assert_eq!(lib.display_name, "Foo library");
    assert_eq!(lib.tags, vec!["library".to_string()]);
    assert_eq!(lib.language_ids, vec!["rust".to_string()]);
    let lib_caps = lib.capabilities.as_ref().expect("lib capabilities");
    assert!(lib_caps.can_compile, "declared compile");
    assert!(lib_caps.can_test, "declared test");
    assert!(!lib_caps.can_run, "declared no run");

    // rust_binary → capabilities derived: compile + test + run; tag application; language rust.
    let app = find(&targets, "packages/foo:app");
    assert_eq!(app.tags, vec!["application".to_string()]);
    assert_eq!(app.language_ids, vec!["rust".to_string()]);
    let app_caps = app.capabilities.as_ref().expect("app capabilities");
    assert!(app_caps.can_compile && app_caps.can_test && app_caps.can_run);

    // docker_image → compile-only, tag application.
    let image = find(&targets, "packages/foo:image");
    assert_eq!(image.tags, vec!["application".to_string()]);
    let image_caps = image.capabilities.as_ref().expect("image capabilities");
    assert!(image_caps.can_compile);
    assert!(
        !image_caps.can_test,
        "docker image derives no test capability"
    );
    assert!(
        !image_caps.can_run,
        "docker image derives no run capability"
    );
}

#[tokio::test]
async fn build_target_sources_returns_the_source_globs_of_a_target() {
    // Given
    let bsp = a_bsp_service_over(THREE_TARGETS);

    // When
    let sources = bsp.sources("packages/foo:lib").await;

    // Then — the target's declared srcs surface as source items.
    assert!(
        sources.contains(&"packages/foo/src/lib.rs".to_string()),
        "expected lib.rs among sources, got {sources:?}"
    );
    assert!(
        sources.contains(&"packages/foo/Cargo.toml".to_string()),
        "expected Cargo.toml among sources, got {sources:?}"
    );
}

#[tokio::test]
async fn build_target_compile_runs_the_targets_compile_action() {
    // Given
    let bsp = a_bsp_service_over(THREE_TARGETS);

    // When — a dry-run compile plans the action without executing cargo.
    let resp = bsp.compile("packages/foo:lib", true).await;

    // Then
    assert_eq!(resp.results.len(), 1);
    let result = &resp.results[0];
    assert_eq!(
        result.status, "ok",
        "compile status; error was {:?}",
        result.error_message
    );
    let argv = &result.actions.first().expect("at least one action").argv;
    assert!(
        argv.windows(3).any(|w| w == ["build", "-p", "foo"]),
        "expected a cargo build -p foo invocation, got {argv:?}"
    );
}

#[tokio::test]
async fn build_target_test_is_rejected_for_a_target_without_the_test_capability() {
    // Given — the docker image target derives no test capability.
    let bsp = a_bsp_service_over(THREE_TARGETS);

    // When
    let resp = bsp.test("packages/foo:image").await;

    // Then — the target is rejected with a clear error and no actions run.
    assert_eq!(resp.results.len(), 1);
    let result = &resp.results[0];
    assert_eq!(result.status, "error");
    assert!(
        result.error_message.to_lowercase().contains("test"),
        "error should mention the unsupported test capability, got {:?}",
        result.error_message
    );
    assert!(result.actions.is_empty(), "nothing should have run");
}

#[tokio::test]
async fn workspace_reload_reflects_a_changed_build_yaml() {
    // Given — a repo initially declaring a single target.
    let one_target = "\
schema_version: 1
targets:
  - id: \"packages/foo:lib\"
    name: Foo library
    config: { type: rust_library, package: foo }
";
    let bsp = a_bsp_service_over(one_target);
    let before = bsp.workspace_build_targets().await;
    assert_eq!(ids(&before), vec!["packages/foo:lib"]);

    // When — the manifest gains a second target and the workspace is reloaded.
    bsp.rewrite_build_yaml(THREE_TARGETS);
    bsp.reload().await;

    // Then — the reloaded catalog reflects all three targets.
    let reloaded = bsp.workspace_build_targets().await;
    let mut after = ids(&reloaded);
    after.sort_unstable();
    assert_eq!(
        after,
        vec!["packages/foo:app", "packages/foo:image", "packages/foo:lib"]
    );
}

//! Loaded plans stay current across plans, through the daemon: an operation applied from one
//! loaded plan is folded into every other loaded plan of the root, and a plan nobody loaded is left
//! alone.
//!
//! Over the deterministic fake language server: `move_test_binary_to_crate` is authored without
//! asking the server anything, so the cross-plan bookkeeping is what these exercise — the
//! re-resolution through a real outline is `tddy-code-restructuring`'s `live_plans_acceptance`.

use std::path::Path;
use std::time::Duration;

use prost::Message;
use tddy_index_daemon::proto::code_index::{
    ApplyRequest, LoadPlansRequest, PlansResponse, RestructureEvent,
};
use tddy_index_daemon::{build_code_index_entry, CodeIndexPorts};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspRegistry};
use tddy_task::TaskRegistry;

const GOLDEN: &str = "crates/origin/tests/golden.rs";
const MOVED_GOLDEN: &str = "crates/destination/tests/golden.rs";

fn a_host_over_fake_language_servers() -> tddy_rpc::ServiceEntry {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
            .with_capabilities(tddy_code_restructuring::client_capabilities())
            .with_initialization_options(tddy_code_restructuring::server_settings()),
    );
    build_code_index_entry(CodeIndexPorts {
        servers: LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60)),
    })
}

/// Two crates and a test binary in `origin` that stands alone, committed — so moving it to
/// `destination` compiles and passes the post-apply gate.
fn a_workspace_whose_test_binary_stands_alone() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    for (path, text) in [
        (
            "Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/origin\", \"crates/destination\"]\n",
        ),
        (
            "crates/origin/Cargo.toml",
            "[package]\nname = \"origin\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/origin/src/lib.rs", "pub fn one() -> u32 {\n    1\n}\n"),
        (GOLDEN, "#[test]\nfn golden() {\n    assert_eq!(1, 1);\n}\n"),
        (
            "crates/destination/Cargo.toml",
            "[package]\nname = \"destination\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("crates/destination/src/lib.rs", ""),
    ] {
        let absolute = workspace.path().join(path);
        std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        std::fs::write(absolute, text).unwrap();
    }
    for args in [
        vec!["init", "--quiet"],
        vec!["add", "--all"],
        vec![
            "-c",
            "user.name=tddy tests",
            "-c",
            "user.email=tests@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "--message",
            "baseline",
        ],
    ] {
        let status = std::process::Command::new("git")
            .args(&args)
            .current_dir(workspace.path())
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }
    workspace
}

fn a_plan(workspace: &tempfile::TempDir, name: &str, op: &str) -> std::path::PathBuf {
    let plan = workspace.path().join(name);
    std::fs::write(&plan, format!("{{\"v\":1,\"snapshot\":{{}}}}\n{op}\n"))
        .expect("write the plan");
    plan
}

const MOVE_GOLDEN: &str =
    "{\"id\":\"a1\",\"op\":\"move_test_binary_to_crate\",\"anchor\":{\"kind\":\"symbol\",\
                           \"file\":\"crates/origin/tests/golden.rs\",\"path\":\"golden\"},\
                           \"to\":\"crates/destination\"}";
const RENAME_IN_GOLDEN: &str =
    "{\"id\":\"b1\",\"op\":\"rename_symbol\",\"anchor\":{\"kind\":\"symbol\",\
                                \"file\":\"crates/origin/tests/golden.rs\",\"path\":\"golden\"},\
                                \"name\":\"golden_values\"}";

async fn unary_at<Req: Message, Res: Message + Default>(
    entry: &tddy_rpc::ServiceEntry,
    method: &str,
    request: Req,
) -> Result<Res, tddy_rpc::Status> {
    match entry
        .service
        .handle_rpc(entry.name, method, &a_message_carrying(request))
        .await
    {
        tddy_rpc::RpcResult::Unary(Ok(payload)) => {
            Ok(Res::decode(payload.as_slice()).expect("the answer decodes"))
        }
        tddy_rpc::RpcResult::Unary(Err(status)) => Err(status),
        tddy_rpc::RpcResult::ServerStream(_) => panic!("expected a unary answer from {method}"),
    }
}

/// Apply `plan` through the daemon and drain its events — its outcome is not what these assert.
async fn applying(entry: &tddy_rpc::ServiceEntry, root: &Path, plan: &Path) {
    let request = ApplyRequest {
        workspace_root: root.to_string_lossy().to_string(),
        plan: plan.to_string_lossy().to_string(),
        dry_run: false,
        resume: false,
        from: None,
        stop_after: None,
    };
    if let tddy_rpc::RpcResult::ServerStream(Ok(mut receiver)) = entry
        .service
        .handle_rpc(entry.name, "Apply", &a_message_carrying(request))
        .await
    {
        while let Some(next) = receiver.recv().await {
            let _ = next.map(|payload| RestructureEvent::decode(payload.as_slice()));
        }
    }
}

fn a_message_carrying<Req: Message>(request: Req) -> tddy_rpc::RpcMessage {
    tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_test_binary_move_in_plan_a_moves_plan_bs_file_hint() {
    // Given two loaded plans: `first` moves the test binary `second` renames inside
    let workspace = a_workspace_whose_test_binary_stands_alone();
    let first = a_plan(&workspace, "first.jsonl", MOVE_GOLDEN);
    let second = a_plan(&workspace, "second.jsonl", RENAME_IN_GOLDEN);
    let entry = a_host_over_fake_language_servers();
    let _: PlansResponse = unary_at(
        &entry,
        "LoadPlans",
        LoadPlansRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
            plans: vec!["first.jsonl".to_string(), "second.jsonl".to_string()],
        },
    )
    .await
    .expect("both plans load");

    // When `first` is applied
    applying(&entry, workspace.path(), &first).await;

    // Then `second`, written back, names the test binary where it now is
    let written = tddy_code_restructuring::Plan::parse(&std::fs::read_to_string(&second).unwrap())
        .expect("the second plan parses");
    assert_eq!(written.ops[0].anchor.file(), MOVED_GOLDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unloaded_plan_is_byte_identical_after_another_plan_applies() {
    // Given one loaded plan that moves the test binary, and one plan nobody loaded that names it
    let workspace = a_workspace_whose_test_binary_stands_alone();
    let first = a_plan(&workspace, "first.jsonl", MOVE_GOLDEN);
    let unheld = a_plan(&workspace, "unheld.jsonl", RENAME_IN_GOLDEN);
    let before = std::fs::read_to_string(&unheld).unwrap();
    let entry = a_host_over_fake_language_servers();

    // When the loaded plan is applied
    applying(&entry, workspace.path(), &first).await;

    // Then the unloaded plan is exactly as it was
    assert_eq!(std::fs::read_to_string(&unheld).unwrap(), before);
}

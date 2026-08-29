//! PRD acceptance ([docs/ft/coder/pr-stack-docs.md] § Artifact path correction): the two plan
//! artifacts live under the session **artifacts root**, alongside `exploration.md` and the two
//! `stack-status.*` files.
//!
//! Every reader but one resolves a manifest basename under `artifacts/` — `context_docs_for_session`,
//! `read_session_context_doc_utf8`, and `resolve_host_document`'s `SESSION_ARTIFACT` root. Written to
//! the session root, `stack-plan.yaml` and `pr-stack-plan.md` therefore report `exists: false` on the
//! wire and cannot be referenced as host documents at all — which is what blocks attaching
//! `pr-stack-plan.md` to a child session.
//!
//! The one reader that does find them, `build_context_header`, probes `artifacts/` and falls back to
//! the session root. That fallback is what makes the move migration-free, so it is pinned here too.

use std::fs;
use std::path::{Path, PathBuf};

use tddy_core::changeset::{write_changeset, Changeset};
use tddy_core::workflow::build_context_header;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::hooks::RunnerHooks;
use tddy_core::workflow::task::{NextAction, TaskResult};
use tddy_workflow_recipes::pr_stack::{PrStackHooks, PrStackRecipe};
use tddy_workflow_recipes::session_artifact_manifest::SessionArtifactManifest;

const STACK_PLAN: &str = "stack-plan.yaml";
const STACK_PLAN_MD: &str = "pr-stack-plan.md";

/// An orchestrator session directory holding the `changeset.yaml` every stack writer reads.
fn an_orchestrator_session() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp session dir");
    write_changeset(tmp.path(), &Changeset::default()).expect("seed changeset");
    tmp
}

/// A valid `write-stack-plan` submission — one root PR under a single `feature/<stack>/` namespace.
fn a_stack_plan_submit() -> String {
    r#"version: 1
prs:
  - node_id: n1
    title: Token store
    description: persist and read auth tokens
    branch_suggestion: feature/auth/token-store
    parents: []
"#
    .to_string()
}

/// Drive the real `after_task` seam rather than the private writer, so the test pins the behaviour a
/// running session actually gets.
fn write_stack_plan_completes(session_dir: &Path, submit_yaml: String) {
    let context = Context::new();
    context.set_sync("session_dir", session_dir.to_path_buf());
    context.set_sync("output", submit_yaml);

    let result = TaskResult {
        response: String::new(),
        next_action: NextAction::Continue,
        task_id: "write-stack-plan".to_string(),
        status_message: None,
    };

    PrStackHooks::new(None)
        .after_task("write-stack-plan", &context, &result)
        .expect("write-stack-plan after_task");
}

trait SessionArtifactAssertions {
    fn assert_artifact(&self, basename: &str) -> &Self;
    fn assert_nothing_at_session_root(&self, basename: &str) -> &Self;
}

impl SessionArtifactAssertions for Path {
    fn assert_artifact(&self, basename: &str) -> &Self {
        let path = self.join("artifacts").join(basename);
        assert!(
            path.is_file(),
            "expected '{basename}' under the artifacts root at {}",
            path.display()
        );
        self
    }

    fn assert_nothing_at_session_root(&self, basename: &str) -> &Self {
        let path = self.join(basename);
        assert!(
            !path.exists(),
            "'{basename}' must not be left at the session root — every wire reader resolves \
             manifest basenames under artifacts/, so a copy here is invisible to them; found {}",
            path.display()
        );
        self
    }
}

#[test]
fn the_stack_plan_and_its_markdown_are_persisted_under_the_artifacts_root() {
    // Given — an orchestrator session with no artifacts yet
    let session = an_orchestrator_session();

    // When — the agent completes its write-stack-plan submit
    write_stack_plan_completes(session.path(), a_stack_plan_submit());

    // Then — both plan artifacts sit beside exploration.md and the status rollups
    session
        .path()
        .assert_artifact(STACK_PLAN)
        .assert_artifact(STACK_PLAN_MD);
}

#[test]
fn the_plan_artifacts_are_not_left_at_the_session_root() {
    // Given
    let session = an_orchestrator_session();

    // When
    write_stack_plan_completes(session.path(), a_stack_plan_submit());

    // Then — a second copy at the root would drift from the one clients read
    session
        .path()
        .assert_nothing_at_session_root(STACK_PLAN)
        .assert_nothing_at_session_root(STACK_PLAN_MD);
}

#[test]
fn the_persisted_markdown_renders_the_submitted_plan() {
    // Given
    let session = an_orchestrator_session();

    // When
    write_stack_plan_completes(session.path(), a_stack_plan_submit());

    // Then — moving the file must not change what it holds
    let md = fs::read_to_string(session.path().join("artifacts").join(STACK_PLAN_MD))
        .expect("read pr-stack-plan.md");
    assert!(
        md.contains("## n1 — Token store"),
        "the plan markdown must render the node heading; got:\n{md}"
    );
    assert!(
        md.contains("feature/auth/token-store"),
        "the plan markdown must name the suggested branch; got:\n{md}"
    );
}

/// Regression guard, not a new behaviour: `build_context_header` probes `artifacts/<name>` and falls
/// back to the session root. That fallback is the entire migration story for orchestrators created
/// before the move — without it, moving the write path would strand every existing session's plan.
#[test]
fn a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent() {
    // Given — an orchestrator whose plan was written before the artifacts move
    let session = an_orchestrator_session();
    let legacy_plan = session.path().join(STACK_PLAN_MD);
    fs::write(&legacy_plan, "# PR Stack Plan\n").expect("write legacy plan");

    // When — the orchestrate goal builds its context header
    let basenames = PrStackRecipe.context_header_filenames();
    let header = build_context_header(Some(session.path()), None, &basenames);

    // Then — the agent is still told where its plan is
    let expected = format!("{STACK_PLAN_MD}: {}", legacy_plan.display());
    assert!(
        header.contains(&expected),
        "the context header must fall back to the session root for a pre-move plan; \
         expected a line '{expected}' in:\n{header}"
    );
}

/// The move exists so that this reference resolves. `SESSION_ARTIFACT` roots at
/// `session_dir/artifacts`, so a plan at the session root is unreachable by relative path — which is
/// what a child session's attachment has to name.
#[test]
fn the_plan_markdown_is_reachable_by_a_path_relative_to_the_artifacts_root() {
    // Given
    let session = an_orchestrator_session();

    // When
    write_stack_plan_completes(session.path(), a_stack_plan_submit());

    // Then — the exact join a SESSION_ARTIFACT reference performs
    let artifacts_root: PathBuf = session.path().join("artifacts");
    let referenced = artifacts_root.join(STACK_PLAN_MD);
    assert!(
        referenced.is_file(),
        "a SESSION_ARTIFACT reference to '{STACK_PLAN_MD}' must resolve to a file; \
         looked at {}",
        referenced.display()
    );
}

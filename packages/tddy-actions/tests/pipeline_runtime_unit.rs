//! Unit tests for `PipelineRuntime` — mapper → primary → transform with schema validation.

use std::collections::BTreeMap;

use serde_json::json;
use tddy_actions::{
    ActionSpec, ChannelMode, PipelineRuntime, PipelineSpec, PipelineStage, SessionActionExtras,
};
use tddy_task::{TaskRegistry, TaskStatus};

#[tokio::test]
async fn pipeline_action_runs_mapper_primary_transform_and_validates_output() {
    // Given — mapper emits argv envelope; primary prints bytes; transform emits schema-valid JSON
    let spec = ActionSpec {
        id: "pipeline-probe".into(),
        kind: "pipeline-action".into(),
        command: vec![],
        inputs: vec![],
        outputs: vec![],
        env: BTreeMap::new(),
        working_dir: None,
        channel_mode: ChannelMode::StdoutStderr,
        sandbox: None,
        session: Some(SessionActionExtras {
            summary: "pipeline probe".into(),
            architecture: "native".into(),
            input_schema: None,
            output_schema: Some(json!({
                "type": "object",
                "required": ["status"],
                "properties": { "status": { "type": "string" } },
                "additionalProperties": false
            })),
            result_kind: None,
            output_path_arg: None,
            manifest_version: 1,
        }),
        pipeline: Some(PipelineSpec {
            input_mapper: Some(PipelineStage {
                program: "/bin/sh".into(),
                args: vec![
                    "-c".into(),
                    r#"printf '%s\n' '{"args":["/bin/sh","-c","printf pipeline_ok"],"env":{}}'"#
                        .into(),
                ],
                env: BTreeMap::new(),
            }),
            primary: PipelineStage {
                program: "/bin/true".into(),
                args: vec![],
                env: BTreeMap::new(),
            },
            output_transform: Some(PipelineStage {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), r#"printf '%s\n' '{"status":"ok"}'"#.into()],
                env: BTreeMap::new(),
            }),
            capture_channel_ids: vec!["stdout".into(), "stderr".into()],
        }),
    };

    let registry = TaskRegistry::new();
    let handle = PipelineRuntime::spawn(&registry, spec, "pipeline-session")
        .await
        .expect("pipeline spawn");

    // When — wait for terminal status
    wait_terminal(&handle).await;

    // Then — completed with captured primary stdout on the stdout channel
    assert!(
        matches!(
            handle.status(),
            TaskStatus::Completed { exit_code: Some(0) }
        ),
        "pipeline must complete successfully; status={:?}",
        handle.status()
    );
    let stdout = handle
        .channel("stdout")
        .expect("stdout channel")
        .replay_capture();
    assert!(
        String::from_utf8_lossy(&stdout).contains("pipeline_ok"),
        "primary stdout must be captured on stdout channel"
    );
}

async fn wait_terminal(handle: &tddy_task::TaskHandle) {
    let mut rx = handle.status_watch();
    loop {
        if rx.borrow().is_terminal() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

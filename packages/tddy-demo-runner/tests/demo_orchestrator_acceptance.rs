//! Acceptance test: demo orchestrator port-forward path.
//!
//! Uses `MockVm` and an in-test `TelegramNotifier` to assert:
//! - deploy steps are called in order with the recipe's deploy_steps
//! - verify is called with the recipe's verify_command
//! - the forward is requested for the recipe's app port
//! - a `http://localhost:<host_port>` share_url is returned
//! - the link is posted to every configured chat_id

use std::sync::Arc;
use tddy_demo_runner::{BootCall, DemoOrchestrator, RunningVm, TelegramNotifier};
use tddy_vm::MockVm;
use tddy_workflow_recipes::parser::{DemoMode, DemoPlan, DemoStep, PortMap};

// ── in-test Telegram notifier ────────────────────────────────────────────────

/// Minimal test-local notifier that records (chat_id, message) pairs.
struct RecordingNotifier {
    messages: std::sync::Mutex<Vec<(i64, String)>>,
}

impl RecordingNotifier {
    fn new() -> Self {
        Self {
            messages: std::sync::Mutex::new(vec![]),
        }
    }

    fn messages(&self) -> Vec<(i64, String)> {
        self.messages.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl TelegramNotifier for RecordingNotifier {
    async fn send(&self, chat_id: i64, text: &str) -> Result<(), String> {
        self.messages
            .lock()
            .unwrap()
            .push((chat_id, text.to_string()));
        Ok(())
    }
}

// ── recipe fixture ───────────────────────────────────────────────────────────

fn port_forward_recipe() -> DemoPlan {
    DemoPlan {
        demo_type: "web_app".to_string(),
        setup_instructions: "Deploy and run".to_string(),
        steps: vec![DemoStep {
            description: "Open dashboard".to_string(),
            command_or_action: "open http://localhost:8080".to_string(),
            expected_result: "Dashboard loads".to_string(),
        }],
        verification: "HTTP 200 on /health".to_string(),
        mode: Some(DemoMode::PortForward),
        hostfwd: vec![PortMap {
            host_port: 8080,
            guest_port: 80,
        }],
        deploy_steps: vec![
            "apt install -y myapp".to_string(),
            "systemctl start myapp".to_string(),
        ],
        verify_command: Some("curl -f http://localhost:80/health".to_string()),
        build_target: Some("my-app:qcow2".to_string()),
    }
}

// ── shared fixture ───────────────────────────────────────────────────────────

/// A pre-booted RunningVm as the daemon would provide after `StartDemoVm`.
fn running_vm() -> RunningVm {
    RunningVm {
        ssh_host_port: 2222,
        monitor_socket: "/tmp/tddy-test-monitor.sock".to_string(),
        pid: 12345,
    }
}

// ── acceptance tests ─────────────────────────────────────────────────────────

/// Full port-forward demo: deploy → verify → forward → notify all in order.
/// The orchestrator receives an already-running VM — it must not call boot().
#[tokio::test]
async fn demo_orchestrator_port_forward_deploys_verifies_and_posts_link() {
    let mock_vm = Arc::new(MockVm::new());
    let notifier = Arc::new(RecordingNotifier::new());
    let chat_ids = vec![100_i64, 200_i64];
    let recipe = port_forward_recipe();

    let orchestrator = DemoOrchestrator::new(
        mock_vm.clone(),
        Some(notifier.clone() as Arc<dyn TelegramNotifier>),
        chat_ids.clone(),
    );

    let result = orchestrator
        .run(&recipe, running_vm())
        .await
        .expect("demo orchestrator must succeed for port-forward recipe");

    // ── share URL produced ──
    assert_eq!(
        result.share_url, "http://localhost:8080",
        "share_url must be http://localhost:8080, got: {:?}",
        result.share_url
    );

    // ── deploy called once with recipe steps ──
    let deploy_calls = mock_vm.deploy_calls();
    assert_eq!(
        deploy_calls.len(),
        1,
        "deploy must be called exactly once, got: {}",
        deploy_calls.len()
    );
    assert_eq!(
        deploy_calls[0].steps, recipe.deploy_steps,
        "deploy must receive the exact deploy_steps from the recipe"
    );

    // ── verify called once with recipe command ──
    let verify_calls = mock_vm.verify_calls();
    assert_eq!(
        verify_calls.len(),
        1,
        "verify must be called exactly once, got: {}",
        verify_calls.len()
    );
    assert_eq!(
        verify_calls[0].command,
        recipe.verify_command.as_deref().unwrap(),
        "verify must use the recipe's verify_command"
    );

    // ── forward called for app port ──
    let forward_calls = mock_vm.forward_calls();
    assert_eq!(
        forward_calls.len(),
        1,
        "forward must be called exactly once, got: {}",
        forward_calls.len()
    );
    assert_eq!(forward_calls[0].host_port, 8080);
    assert_eq!(forward_calls[0].guest_port, 80);

    // ── Telegram notified with link for every chat_id ──
    let messages = notifier.messages();
    assert_eq!(
        messages.len(),
        chat_ids.len(),
        "one message per chat_id expected ({} chats), got {} messages",
        chat_ids.len(),
        messages.len()
    );
    for (chat_id, text) in &messages {
        assert!(
            chat_ids.contains(chat_id),
            "message sent to unexpected chat_id {chat_id}"
        );
        assert!(
            text.contains("http://localhost:8080"),
            "Telegram message must contain the share URL, got: {text:?}"
        );
    }

    // ── steps_completed matches deploy steps ──
    assert_eq!(
        result.steps_completed,
        recipe.deploy_steps.len() as u32,
        "steps_completed must equal the number of deploy steps"
    );
}

/// Orchestrator must return an error when the recipe has no `mode` set.
#[tokio::test]
async fn demo_orchestrator_errors_when_mode_is_missing() {
    let mock_vm = Arc::new(MockVm::new());
    let orchestrator = DemoOrchestrator::new(mock_vm, None, vec![]);

    let recipe = DemoPlan {
        demo_type: "unknown".to_string(),
        setup_instructions: String::new(),
        steps: vec![],
        verification: String::new(),
        mode: None,
        hostfwd: vec![],
        deploy_steps: vec![],
        verify_command: None,
        build_target: None,
    };

    let result = orchestrator.run(&recipe, running_vm()).await;
    assert!(
        result.is_err(),
        "orchestrator must return an error when mode is None, got: {:?}",
        result.ok()
    );
}

/// Orchestrator must return an error when verify returns a failing exit code.
#[tokio::test]
async fn demo_orchestrator_errors_when_verify_fails() {
    let mut mock_vm = MockVm::new();
    mock_vm.verify_fails = true;
    let mock_vm = Arc::new(mock_vm);

    let orchestrator = DemoOrchestrator::new(mock_vm, None, vec![]);
    let recipe = port_forward_recipe();

    let result = orchestrator.run(&recipe, running_vm()).await;
    assert!(
        result.is_err(),
        "orchestrator must return an error when verify fails, got: {:?}",
        result.ok()
    );
}

/// The orchestrator must never call `boot()` — the VM is launched from the UI.
/// `run()` receives an already-running `RunningVm`; boot is the daemon's responsibility.
#[tokio::test]
async fn demo_orchestrator_does_not_call_boot_it_receives_running_vm() {
    let mock_vm = Arc::new(MockVm::new());
    let orchestrator = DemoOrchestrator::new(mock_vm.clone(), None, vec![]);
    let recipe = port_forward_recipe();

    // run() will fail (stub), but that's OK — we only care that boot was never called.
    let _ = orchestrator.run(&recipe, running_vm()).await;

    let boot_calls: Vec<BootCall> = mock_vm.boot_calls();
    assert_eq!(
        boot_calls.len(),
        0,
        "orchestrator must not call boot() — VM lifecycle is UI-driven; got {} boot call(s)",
        boot_calls.len()
    );
}

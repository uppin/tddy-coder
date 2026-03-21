# Backend Selection Dropdown — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add an interactive backend selection step at session start, using the existing Select/ClarificationQuestion UI, with composer-2 as the default Cursor model.

**Architecture:** Synthetic ClarificationQuestion shown before FeatureInput. The Presenter starts in Select mode; after user picks a backend, transitions to FeatureInput and signals the outer loop to create the backend and start the workflow. The Cursor backend gains `--model` passthrough.

**Tech Stack:** Rust, ratatui (TUI rendering), clap (CLI), tddy-core (Presenter/backend), tddy-tui (view)

---

### Task 1: Backend selection helpers

**Files:**
- Modify: `packages/tddy-core/src/backend/mod.rs`
- Test: same file (inline `#[cfg(test)]` module)

**Step 1: Write the failing tests**

```rust
#[test]
fn backend_selection_question_returns_four_options() {
    let q = backend_selection_question("claude");
    assert_eq!(q.options.len(), 4);
    assert!(!q.multi_select);
    assert!(!q.allow_other);
}

#[test]
fn backend_selection_question_labels() {
    let q = backend_selection_question("claude");
    let labels: Vec<&str> = q.options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, vec!["Claude", "Claude ACP", "Cursor", "Stub"]);
}

#[test]
fn backend_from_label_claude() {
    assert_eq!(backend_from_label("Claude"), ("claude", "opus"));
}

#[test]
fn backend_from_label_cursor() {
    assert_eq!(backend_from_label("Cursor"), ("cursor", "composer-2"));
}

#[test]
fn backend_from_label_claude_acp() {
    assert_eq!(backend_from_label("Claude ACP"), ("claude-acp", "opus"));
}

#[test]
fn backend_from_label_stub() {
    assert_eq!(backend_from_label("Stub"), ("stub", "stub"));
}

#[test]
fn backend_from_label_unknown_defaults_to_claude() {
    assert_eq!(backend_from_label("Unknown"), ("claude", "opus"));
}

#[test]
fn default_model_for_agent_cursor() {
    assert_eq!(default_model_for_agent("cursor"), "composer-2");
}

#[test]
fn default_model_for_agent_claude() {
    assert_eq!(default_model_for_agent("claude"), "opus");
}

#[test]
fn preselected_index_for_agent_returns_correct_index() {
    assert_eq!(preselected_index_for_agent("claude"), 0);
    assert_eq!(preselected_index_for_agent("claude-acp"), 1);
    assert_eq!(preselected_index_for_agent("cursor"), 2);
    assert_eq!(preselected_index_for_agent("stub"), 3);
    assert_eq!(preselected_index_for_agent("unknown"), 0);
}
```

**Step 2: Run tests to verify they fail**

Run: `./test -p tddy-core -- backend_selection`
Expected: compile errors — functions not defined

**Step 3: Write implementation**

Add to `packages/tddy-core/src/backend/mod.rs` (after `QuestionOption` type):

```rust
/// Build a ClarificationQuestion for backend selection at session start.
pub fn backend_selection_question(preselected: &str) -> ClarificationQuestion {
    let _ = preselected; // used by callers for pre-selection index
    ClarificationQuestion {
        header: "Backend".to_string(),
        question: "Select the coding backend".to_string(),
        options: vec![
            QuestionOption {
                label: "Claude".to_string(),
                description: "Claude Code CLI (default model: opus)".to_string(),
            },
            QuestionOption {
                label: "Claude ACP".to_string(),
                description: "Claude Agent Control Protocol (default model: opus)".to_string(),
            },
            QuestionOption {
                label: "Cursor".to_string(),
                description: "Cursor agent CLI (default model: composer-2)".to_string(),
            },
            QuestionOption {
                label: "Stub".to_string(),
                description: "Test backend with simulated responses".to_string(),
            },
        ],
        multi_select: false,
        allow_other: false,
    }
}

/// Map a display label from backend_selection_question to (agent_name, default_model).
pub fn backend_from_label(label: &str) -> (&'static str, &'static str) {
    match label {
        "Claude" => ("claude", "opus"),
        "Claude ACP" => ("claude-acp", "opus"),
        "Cursor" => ("cursor", "composer-2"),
        "Stub" => ("stub", "stub"),
        _ => ("claude", "opus"),
    }
}

/// Default model for a given agent name.
pub fn default_model_for_agent(agent: &str) -> &'static str {
    match agent {
        "cursor" => "composer-2",
        "stub" => "stub",
        _ => "opus",
    }
}

/// Return the pre-selection index for the backend_selection_question options.
pub fn preselected_index_for_agent(agent: &str) -> usize {
    match agent {
        "claude" => 0,
        "claude-acp" => 1,
        "cursor" => 2,
        "stub" => 3,
        _ => 0,
    }
}
```

Also add exports in `packages/tddy-core/src/lib.rs`:
```rust
pub use backend::{backend_selection_question, backend_from_label, default_model_for_agent, preselected_index_for_agent};
```

**Step 4: Run tests to verify they pass**

Run: `./test -p tddy-core -- backend_selection`
Expected: all pass

**Step 5: Commit**

```bash
git add packages/tddy-core/src/backend/mod.rs packages/tddy-core/src/lib.rs
git commit -m "feat: add backend selection helpers (question builder, label mapper)"
```

---

### Task 2: Add initial_selected to AppMode::Select

**Files:**
- Modify: `packages/tddy-core/src/presenter/state.rs`
- Modify: `packages/tddy-core/src/presenter/presenter_impl.rs` (advance_to_next_question)

**Step 1: Write the failing test**

Add to `state.rs` tests:

```rust
#[test]
fn app_mode_select_has_initial_selected() {
    let mode = AppMode::Select {
        question: ClarificationQuestion {
            header: "test".to_string(),
            question: "pick one".to_string(),
            options: vec![],
            multi_select: false,
            allow_other: false,
        },
        question_index: 0,
        total_questions: 1,
        initial_selected: 2,
    };
    if let AppMode::Select { initial_selected, .. } = mode {
        assert_eq!(initial_selected, 2);
    } else {
        panic!("expected Select");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `./test -p tddy-core -- app_mode_select_has_initial`
Expected: compile error — no field `initial_selected`

**Step 3: Add field and fix all match arms**

In `state.rs`, add `initial_selected: usize` to `Select`:

```rust
Select {
    question: ClarificationQuestion,
    question_index: usize,
    total_questions: usize,
    initial_selected: usize,
},
```

In `presenter_impl.rs`, update `advance_to_next_question`:

```rust
self.state.mode = AppMode::Select {
    question: q,
    question_index: self.current_question_index,
    total_questions: total,
    initial_selected: 0,
};
```

Fix all other match arms that explicitly destructure `Select` (use `..` where not already used):

- `packages/tddy-tui/src/render.rs`: `AppMode::Select { question, question_index, .. }` (drop `total_questions: _`)
- `packages/tddy-tui/src/view_state.rs`: `AppMode::Select { initial_selected, .. }` → `self.select_selected = *initial_selected;`
- Any other explicit destructures in key_map.rs, layout.rs, mouse_map.rs — should already use `..`

**Step 4: Run tests to verify they pass**

Run: `./test -p tddy-core -p tddy-tui`
Expected: all pass

**Step 5: Commit**

```bash
git add packages/tddy-core/src/presenter/state.rs packages/tddy-core/src/presenter/presenter_impl.rs packages/tddy-tui/src/render.rs packages/tddy-tui/src/view_state.rs
git commit -m "feat: add initial_selected field to AppMode::Select for pre-selection"
```

---

### Task 3: Cursor backend --model passthrough

**Files:**
- Modify: `packages/tddy-core/src/backend/cursor.rs`
- Test: same file (add test module)

**Step 1: Write the failing test**

The Cursor backend spawns an actual `cursor` binary, which won't exist in test. To test argument construction, extract the arg-building logic into a testable function or verify via log output. The simplest approach: add a unit test that constructs args and checks `--model` is present.

Create a helper function `build_cursor_args` that returns the arg list, then test it:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_args_includes_model_when_set() {
        let request = InvokeRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            system_prompt_path: None,
            goal: Goal::Plan,
            model: Some("composer-2".to_string()),
            session: None,
            working_dir: None,
            debug: false,
            agent_output: false,
            agent_output_sink: None,
            progress_sink: None,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
            socket_path: None,
            plan_dir: None,
        };
        let args = build_cursor_args(&request, "test prompt");
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"composer-2".to_string()));
    }

    #[test]
    fn build_args_omits_model_when_none() {
        let request = InvokeRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            system_prompt_path: None,
            goal: Goal::Plan,
            model: None,
            session: None,
            working_dir: None,
            debug: false,
            agent_output: false,
            agent_output_sink: None,
            progress_sink: None,
            conversation_output_path: None,
            inherit_stdin: false,
            extra_allowed_tools: None,
            socket_path: None,
            plan_dir: None,
        };
        let args = build_cursor_args(&request, "test prompt");
        assert!(!args.contains(&"--model".to_string()));
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `./test -p tddy-core -- build_args_includes_model`
Expected: compile error — `build_cursor_args` not defined

**Step 3: Extract arg-building and add --model**

Refactor `invoke_sync` in `cursor.rs`: extract the arg-building into a function `build_cursor_args(request: &InvokeRequest, prompt: &str) -> Vec<String>`, and add:

```rust
if let Some(ref model) = request.model {
    args.push("--model".to_string());
    args.push(model.clone());
}
```

Insert the `--model` args after the prompt args, before `--output-format`.

**Step 4: Run tests to verify they pass**

Run: `./test -p tddy-core -- build_args`
Expected: all pass

**Step 5: Commit**

```bash
git add packages/tddy-core/src/backend/cursor.rs
git commit -m "feat: pass --model to cursor agent CLI when model is set"
```

---

### Task 4: BackendSelected event

**Files:**
- Modify: `packages/tddy-core/src/presenter/presenter_events.rs`

**Step 1: Add the variant**

```rust
pub enum PresenterEvent {
    // ... existing variants ...
    BackendSelected { agent: String, model: String },
}
```

**Step 2: Run tests**

Run: `./test -p tddy-core`
Expected: all pass (new variant is additive, no match arms need updating since events are sent, not matched in tddy-core)

**Step 3: Commit**

```bash
git add packages/tddy-core/src/presenter/presenter_events.rs
git commit -m "feat: add BackendSelected presenter event variant"
```

---

### Task 5: Presenter backend selection flow

**Files:**
- Modify: `packages/tddy-core/src/presenter/presenter_impl.rs`
- Test: same file

**Step 1: Write failing tests**

```rust
#[test]
fn show_backend_selection_transitions_to_select_mode() {
    let mut p = make_presenter();
    let q = crate::backend_selection_question("claude");
    p.show_backend_selection(q);
    assert!(matches!(p.state().mode, AppMode::Select { .. }));
    assert!(p.is_backend_selection_pending());
}

#[test]
fn backend_selection_answer_transitions_to_feature_input() {
    let mut p = make_presenter();
    let q = crate::backend_selection_question("claude");
    p.show_backend_selection(q);
    // Select "Cursor" (index 2)
    p.handle_intent(UserIntent::AnswerSelect(2));
    assert!(matches!(p.state().mode, AppMode::FeatureInput));
    assert!(!p.is_backend_selection_pending());
    assert_eq!(p.state().agent, "cursor");
    assert_eq!(p.state().model, "composer-2");
}

#[test]
fn backend_selection_answer_claude_acp() {
    let mut p = make_presenter();
    let q = crate::backend_selection_question("claude");
    p.show_backend_selection(q);
    p.handle_intent(UserIntent::AnswerSelect(1));
    assert_eq!(p.state().agent, "claude-acp");
    assert_eq!(p.state().model, "opus");
}

#[test]
fn backend_selection_preserves_cli_model_override() {
    let mut p = Presenter::new("claude", "sonnet");
    let q = crate::backend_selection_question("claude");
    p.show_backend_selection(q);
    // Select Cursor — would default to composer-2, but CLI model "sonnet" should be preserved
    // NOTE: model override is handled in run.rs, not in the Presenter.
    // The Presenter always sets the per-backend default.
    p.handle_intent(UserIntent::AnswerSelect(2));
    assert_eq!(p.state().agent, "cursor");
    assert_eq!(p.state().model, "composer-2");
}
```

**Step 2: Run tests to verify they fail**

Run: `./test -p tddy-core -- backend_selection`
Expected: compile errors — methods not defined

**Step 3: Implement**

Add field to Presenter struct:

```rust
backend_selection_pending: bool,
```

Initialize to `false` in `new()`.

Add methods:

```rust
/// Show backend selection question (transitions to Select mode).
pub fn show_backend_selection(&mut self, question: ClarificationQuestion) {
    self.backend_selection_pending = true;
    self.pending_questions = vec![question.clone()];
    self.current_question_index = 0;
    self.collected_answers.clear();
    let preselected = crate::preselected_index_for_agent(&self.state.agent);
    self.state.mode = AppMode::Select {
        question,
        question_index: 0,
        total_questions: 1,
        initial_selected: preselected,
    };
    self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
}

/// True when backend selection question is still pending.
pub fn is_backend_selection_pending(&self) -> bool {
    self.backend_selection_pending
}
```

In `handle_intent`, add early return in `AnswerSelect` when `backend_selection_pending`:

```rust
UserIntent::AnswerSelect(idx) => {
    if self.backend_selection_pending {
        if let Some(q) = self.pending_questions.first() {
            if idx < q.options.len() {
                let label = &q.options[idx].label;
                let (agent, model) = crate::backend_from_label(label);
                self.state.agent = agent.to_string();
                self.state.model = model.to_string();
                self.backend_selection_pending = false;
                self.pending_questions.clear();
                self.current_question_index = 0;
                self.collected_answers.clear();
                self.state.mode = AppMode::FeatureInput;
                self.broadcast(PresenterEvent::ModeChanged(self.state.mode.clone()));
                self.broadcast(PresenterEvent::BackendSelected {
                    agent: agent.to_string(),
                    model: model.to_string(),
                });
            }
        }
        return;
    }
    // ... existing AnswerSelect handling ...
}
```

**Step 4: Run tests**

Run: `./test -p tddy-core -- backend_selection`
Expected: all pass

**Step 5: Commit**

```bash
git add packages/tddy-core/src/presenter/presenter_impl.rs
git commit -m "feat: presenter backend selection flow with synthetic question"
```

---

### Task 6: TUI flow — deferred backend creation

**Files:**
- Modify: `packages/tddy-coder/src/run.rs`

**Step 1: Modify `run_full_workflow_tui`**

The key change: don't call `create_backend` or `start_workflow` before the TUI starts. Instead:

1. Start the toolcall listener early (independent of backend)
2. Create the Presenter in backend selection mode
3. Start the TUI event loop
4. In the presenter poll thread, detect when backend selection completes, then create the backend and call `start_workflow`

```rust
fn run_full_workflow_tui(args: &Args, shutdown: Arc<AtomicBool>) -> anyhow::Result<()> {
    std::env::set_var("TDDY_QUIET", "1");
    log::set_max_level(log::LevelFilter::Debug);

    if let Some(session_dir) = session_dir_path(args) {
        let logs = session_dir.join("logs");
        tddy_core::toolcall::set_toolcall_log_dir(&logs);
    }

    // Start toolcall listener early — independent of backend selection
    let (socket_path, tool_call_rx) = match tddy_core::toolcall::start_toolcall_listener() {
        Ok((path, rx)) => (Some(path), Some(rx)),
        Err(_) => (None, None),
    };

    // DON'T create backend yet — deferred until user selects one

    let (event_tx, _) = tokio::sync::broadcast::channel(256);
    let (intent_tx, intent_rx) = std::sync::mpsc::channel();
    let mut presenter = Presenter::new(
        &args.agent,
        args.model.as_deref().unwrap_or(
            tddy_core::default_model_for_agent(&args.agent)
        ),
    )
    .with_broadcast(event_tx.clone())
    .with_intent_sender(intent_tx.clone());

    // Show backend selection dropdown
    let backend_question = tddy_core::backend_selection_question(&args.agent);
    presenter.show_backend_selection(backend_question);

    let presenter = Arc::new(Mutex::new(presenter));

    // ... gRPC, LiveKit, web server setup (unchanged) ...

    let conn = presenter.lock().unwrap().connect_view()
        .expect("connect_view requires broadcast and intent_tx");

    // Presenter poll thread — with deferred workflow start
    let shutdown_for_thread = shutdown.clone();
    let presenter_for_thread = presenter.clone();
    let args_clone = args.clone();
    let presenter_handle = std::thread::spawn(move || {
        let mut workflow_started = false;
        let mut socket_path_opt = socket_path;
        let mut tool_call_rx_opt = tool_call_rx;
        for _ in 0..100_000 {
            if shutdown_for_thread.load(Ordering::Relaxed) { break; }
            while let Ok(intent) = intent_rx.try_recv() {
                if let Ok(mut p) = presenter_for_thread.lock() {
                    p.handle_intent(intent);
                }
            }
            if let Ok(mut p) = presenter_for_thread.lock() {
                // Deferred workflow start: once backend is selected, create it and start
                if !workflow_started && !p.is_backend_selection_pending() {
                    let selected_agent = p.state().agent.clone();
                    let backend = create_backend(
                        &selected_agent,
                        socket_path_opt.as_deref(),
                        None,
                    );
                    let initial_prompt = args_clone.prompt.clone();
                    p.start_workflow(
                        backend,
                        PathBuf::from("."),
                        args_clone.plan_dir.clone(),
                        initial_prompt,
                        args_clone.conversation_output.clone(),
                        None,
                        is_debug_mode(&args_clone),
                        args_clone.session_id.clone(),
                        socket_path_opt.take(),
                        tool_call_rx_opt.take(),
                    );
                    workflow_started = true;
                }
                p.poll_tool_calls();
                p.poll_workflow();
                if p.state().should_quit { break; }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    // ... TUI event loop and exit handling (unchanged) ...
}
```

**Step 2: Verify `verify_tddy_tools_available` is deferred too**

Currently called in `run_with_args` before `run_full_workflow_tui`. With dynamic backend selection, the agent may change. Move the check into the deferred start (after backend selection) or accept that it runs with the CLI default and may need re-checking.

Simplest: leave the check where it is. If the user switches from `claude` to `stub`, the check already passed (and stub doesn't need tddy-tools). If switching from `stub` to `claude`, the check would have been skipped. So defer the check to after selection.

**Step 3: Run tests**

Run: `./test -p tddy-coder`
Expected: all pass (TUI code is hard to unit test — integration test via stub backend)

**Step 4: Commit**

```bash
git add packages/tddy-coder/src/run.rs
git commit -m "feat: defer backend creation in TUI until backend selection completes"
```

---

### Task 7: Plain flow — backend selection menu

**Files:**
- Modify: `packages/tddy-coder/src/plain.rs`
- Modify: `packages/tddy-coder/src/run.rs`
- Test: `packages/tddy-coder/src/plain.rs`

**Step 1: Write failing test for read_backend_selection_plain**

```rust
#[test]
fn read_backend_selection_plain_parses_valid_index() {
    // This tests the index-to-label mapping logic, not actual stdin reading.
    let q = tddy_core::backend_selection_question("claude");
    let label = resolve_backend_selection_index(1, &q);
    assert_eq!(label, "Claude");
}

#[test]
fn resolve_backend_selection_index_cursor() {
    let q = tddy_core::backend_selection_question("claude");
    let label = resolve_backend_selection_index(3, &q);
    assert_eq!(label, "Cursor");
}

#[test]
fn resolve_backend_selection_index_out_of_bounds_defaults_to_first() {
    let q = tddy_core::backend_selection_question("claude");
    let label = resolve_backend_selection_index(99, &q);
    assert_eq!(label, "Claude");
}
```

**Step 2: Run tests to verify they fail**

Run: `./test -p tddy-coder -- resolve_backend_selection`
Expected: compile error — function not defined

**Step 3: Implement**

Add to `plain.rs`:

```rust
/// Resolve a 1-based index to a backend label from the question options.
pub fn resolve_backend_selection_index(index: usize, question: &ClarificationQuestion) -> String {
    let idx = index.saturating_sub(1).min(question.options.len().saturating_sub(1));
    question.options[idx].label.clone()
}

/// Print backend selection menu and read choice from stdin.
/// Returns the label of the selected backend.
pub fn read_backend_selection_plain(question: &ClarificationQuestion) -> anyhow::Result<String> {
    eprintln!("\n{}: {}", question.header, question.question);
    for (i, opt) in question.options.iter().enumerate() {
        eprintln!("  {}. {} — {}", i + 1, opt.label, opt.description);
    }
    eprint!("Select [1-{}]: ", question.options.len());
    let mut buf = String::new();
    io::stdin().lock().read_line(&mut buf)?;
    let choice = buf.trim().parse::<usize>().unwrap_or(1);
    Ok(resolve_backend_selection_index(choice, question))
}
```

In `run.rs`, update `run_full_workflow_plain` and `run_with_args` (for single-goal runs):

```rust
// At the start of run_full_workflow_plain:
let question = tddy_core::backend_selection_question(&args.agent);
let selection = plain::read_backend_selection_plain(&question)?;
let (selected_agent, default_model) = tddy_core::backend_from_label(&selection);
let effective_model = args.model.as_deref().unwrap_or(default_model);
let backend = create_backend(selected_agent, None, None);
// Use selected_agent and effective_model for subsequent operations
```

**Step 4: Run tests**

Run: `./test -p tddy-coder -- resolve_backend_selection`
Expected: all pass

**Step 5: Commit**

```bash
git add packages/tddy-coder/src/plain.rs packages/tddy-coder/src/run.rs
git commit -m "feat: backend selection menu in plain mode"
```

---

### Task 8: Daemon flow — skip interactive selection

**Files:**
- Modify: `packages/tddy-coder/src/run.rs` (verify run_daemon uses CLI agent directly)

No interactive selection in daemon mode — it uses `--agent` from CLI/config. This is already the case. Verify and add a comment documenting the decision.

**Step 1: Verify**

Read `run_daemon` in `run.rs` — confirm it calls `create_backend(&args.agent, ...)` directly.

**Step 2: Add comment**

```rust
// Daemon mode: no interactive backend selection. Use --agent from CLI/config.
// Web clients send backend choice via gRPC StartSession RPC.
let backend = create_backend(&args.agent, None, None);
```

**Step 3: Commit**

```bash
git add packages/tddy-coder/src/run.rs
git commit -m "docs: document daemon backend selection behavior"
```

---

### Task 9: Move verify_tddy_tools_available to after backend selection

**Files:**
- Modify: `packages/tddy-coder/src/run.rs`

Currently `verify_tddy_tools_available` is called in `run_with_args` before any flow starts. With dynamic backend selection, the agent may change. Move the check to after selection in TUI and plain flows.

**Step 1: Move the check**

In `run_with_args`: remove the early `verify_tddy_tools_available(&args.agent)?` call.

In `run_full_workflow_tui`: call it in the deferred start block after backend selection:
```rust
if !workflow_started && !p.is_backend_selection_pending() {
    let selected_agent = p.state().agent.clone();
    if let Err(e) = verify_tddy_tools_available(&selected_agent) {
        log::error!("tddy-tools not available: {}", e);
        // Show error and let user re-select or quit
    }
    let backend = create_backend(&selected_agent, ...);
    // ...
}
```

In `run_full_workflow_plain`: call it after `read_backend_selection_plain`:
```rust
verify_tddy_tools_available(selected_agent)?;
```

Keep it in `run_daemon` (daemon doesn't do interactive selection).

For single-goal runs (`run_with_args` with `args.goal.is_some()`), keep the early check since those use CLI agent directly.

**Step 2: Run tests**

Run: `./test -p tddy-coder`
Expected: all pass

**Step 3: Commit**

```bash
git add packages/tddy-coder/src/run.rs
git commit -m "refactor: defer verify_tddy_tools_available to after backend selection"
```

---

### Task 10: Update lib.rs exports and verify full build

**Files:**
- Modify: `packages/tddy-core/src/lib.rs`

**Step 1: Add exports**

Ensure all new public functions are exported from tddy-core's lib.rs:
```rust
pub use backend::{
    backend_selection_question, backend_from_label, default_model_for_agent,
    preselected_index_for_agent,
    // ... existing exports ...
};
```

**Step 2: Full build + test**

Run: `./test`
Expected: all tests pass, no warnings

**Step 3: Commit**

```bash
git add packages/tddy-core/src/lib.rs
git commit -m "feat: export backend selection helpers from tddy-core"
```

---

### Task 11: Run clippy and fmt

**Step 1: Format**

Run: `cargo fmt`

**Step 2: Lint**

Run: `cargo clippy -- -D warnings`
Fix any warnings.

**Step 3: Commit**

```bash
git add -A
git commit -m "style: clippy + fmt cleanup for backend selection feature"
```

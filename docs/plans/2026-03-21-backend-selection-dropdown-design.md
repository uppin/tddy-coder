# Backend Selection Dropdown — Design

## Problem

When a session starts, the coding backend (Claude, Claude ACP, Cursor, Stub) is determined solely by the `--agent` CLI flag. There is no interactive way to pick or change the backend at session startup. Additionally, the Cursor backend ignores the model field — it logs it but never passes `--model` to the `cursor agent` CLI.

## Solution

Add a backend selection step at the very beginning of every session, before the feature input prompt. This uses the existing `AppMode::Select` and `ClarificationQuestion` UI infrastructure — no new AppMode variant needed.

### Session lifecycle change

```
Select (backend question) → FeatureInput → Running → ... → Done
```

### Backend options

| Display label | Agent name | Default model |
|--------------|-----------|---------------|
| Claude | `claude` | `opus` |
| Claude ACP | `claude-acp` | `opus` |
| Cursor | `cursor` | `composer-2` |
| Stub | `stub` | (none) |

### Pre-selection

The `--agent` CLI value determines which option is pre-highlighted. The dropdown always appears; the CLI value is the default, not a skip.

### Model precedence

1. Explicit `--model` CLI flag (highest priority)
2. Per-backend default from the table above
3. "opus" (fallback)

## Architecture

### Deferred workflow start

Today the backend is created and `start_workflow()` is called before the TUI event loop. With backend selection in the TUI, both are deferred:

1. Presenter starts in `Select` mode with a synthetic backend `ClarificationQuestion`
2. User picks backend via Up/Down/Enter
3. Presenter stores selection, transitions to `FeatureInput`, broadcasts `BackendSelected`
4. The outer event loop detects the selection, creates the backend, and calls `start_workflow()`

### Per-flow behavior

**TUI** (`run_full_workflow_tui`): Renders the Select screen, user navigates and picks. After selection, backend is created and workflow starts.

**Plain** (`run_full_workflow_plain`): Prints a numbered menu to stderr, reads selection from stdin. Then proceeds with normal flow.

**Daemon** (`run_daemon`): No interactive selection — uses `--agent` directly. Web clients can send backend selection via gRPC `StartSession`.

### Presenter changes

- New field `backend_selection_pending: bool` on `Presenter`
- `new()` starts with `Select` mode containing the backend question
- `AnswerSelect` while `backend_selection_pending`: extracts `(agent, model)`, updates state, clears flag, transitions to `FeatureInput`, broadcasts `BackendSelected { agent, model }`

### Cursor backend model passthrough

When `request.model` is `Some(m)`, the Cursor backend adds `--model` and the model name to the `cursor agent` CLI args — matching what the Claude backend already does.

## Files changed

| File | Change |
|------|--------|
| `packages/tddy-core/src/backend/mod.rs` | `backend_selection_question()`, `backend_from_label()` |
| `packages/tddy-core/src/backend/cursor.rs` | Pass `--model` to cursor CLI |
| `packages/tddy-core/src/presenter/presenter_impl.rs` | Backend selection handling, deferred FeatureInput |
| `packages/tddy-core/src/presenter/presenter_events.rs` | `BackendSelected` event variant |
| `packages/tddy-coder/src/run.rs` | Deferred backend creation in TUI; backend menu in plain |

## Decisions made

- **Dropdown always appears** — even when `--agent` is passed, it's pre-selected but user can change
- **All flows** — TUI, plain, and daemon all support backend selection (daemon via CLI)
- **Approach 2 chosen** — reuse existing `Select` / `ClarificationQuestion` infrastructure rather than adding a new `AppMode::BackendSelect`
- **Cursor model passthrough** — pass `--model` to `cursor agent` CLI so `composer-2` takes effect

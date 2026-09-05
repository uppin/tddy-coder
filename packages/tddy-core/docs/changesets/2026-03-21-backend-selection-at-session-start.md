# 2026-03-21 — Backend selection at session start

**Type:** Feature

`backend_selection_question`, `backend_from_label`, `default_model_for_agent`, `preselected_index_for_agent`; `CursorBackend` passes `--model` to `cursor agent` when set; `AppMode::Select` adds `initial_selected`; presenter `show_backend_selection` / `PendingWorkflowStart` / `DeferredBackendFactory`; `PresenterEvent::BackendSelected { agent, model }`; `AnswerSelect` starts workflow with chosen backend. (tddy-core)

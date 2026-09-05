# 2026-05-02 — Session workflow action cache

**Type:** Feature

**`workflow::action_cache`**: **`action_cache_file_path`**, **`lookup_cached_completed_submit`**, **`persist_successful_submit_to_action_cache`**, **`fingerprint_action_inputs`**, **`stable_action_cache_key`**, **`action_cache_disabled`**; **`BackendInvokeTask`** lookup before invoke and persist after **`submit`**; **`FlowRunner`** graph/task context keys; **`CodingBackend::action_invoke_cache_eligible`** (**`MockBackend`** **false**). Feature doc: [session-actions.md](../../../../../docs/ft/coder/session-actions.md) (Workflow action cache); architecture: [architecture.md](../../architecture.md#workflow-action-cache). Tests: **`workflow::action_cache`** (lib); **`tddy-integration-tests`** **`workflow_graph`** **`action_cache_*`**. (tddy-core)

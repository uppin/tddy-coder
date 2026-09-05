# 2026-03-14 — Workflow Restart on Completion

**Type:** Feature

WorkflowComplete handler transitions to FeatureInput instead of Done when inbox empty. SubmitFeatureInput detects dead channel via send() failure, calls restart_workflow(). is_done() checks workflow_result.is_some(). Inbox dequeue clears workflow_result. Unit test: success → FeatureInput. (tddy-core)

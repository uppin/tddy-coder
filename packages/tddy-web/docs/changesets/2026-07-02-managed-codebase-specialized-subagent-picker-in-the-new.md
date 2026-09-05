# 2026-07-02 — "Managed codebase" specialized-subagent picker in the new-session form

**Type:** Feature

`CreateSessionPane` gains a collapsible "Managed codebase" section (claude-cli sessions only, `data-testid="create-session-managed-codebase-toggle"`/`-section`) listing subagents from the new `listSubagents` RPC as checkboxes (`create-session-subagent-checkbox-<name>`); `managedCodebase` is derived from `selectedSubagents.length > 0` and both fields are threaded into the claude-cli `startSession` call. Component test `CreateSessionManagedCodebase.cy.tsx` (in-memory backend). Feature [specialized-subagents.md](../../../../docs/ft/coder/specialized-subagents.md). (tddy-web, tddy-service, tddy-daemon)

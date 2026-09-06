# 2026-08-30 — `SessionMainPane` and `SessionRuntime` are over the file-size guideline

**Category:** Future enhancement
**Source:** session-agent-conversation-tab changeset, 2026-08-30

- Both were over before that change (539 → 560 and 515 → 587); it added ~20 and ~70 lines and deleted
  ~80 of peer-spawn machinery from the first. Flagged rather than acted on so the feature diff stayed
  reviewable.
- `SessionMainPane`: the cheapest extraction is `useSessionAgentConversations` — the two per-session
  maps plus `attachAgent` / `focusConversation` / `closeConversation`, ~85 lines. No call site moves,
  no test repoints (every spec drives the DOM through `testIds.ts`), and it lands the file at ~478.
- `SessionRuntime`: `useRuntimeFocusGuard` (the `focusin` steal guard and focus-on-select effect) and
  `useSessionRuntimeClients` (the `buildSessionClient` / lease / terminal-client memos), ~90 lines
  together, no shared state to widen. Do **not** split `SessionChildRuntime` out: it renders
  `SessionRuntime`, which renders it, and separating them creates an import cycle that resolves at
  runtime but trips `import/no-cycle`.
- `CreateSessionPane` is 1319 lines and wants the same treatment, but under a six-spec Cypress test-id
  contract a structural move should be the only thing in its diff.

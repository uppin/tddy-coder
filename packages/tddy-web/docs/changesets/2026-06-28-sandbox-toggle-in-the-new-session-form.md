# 2026-06-28 — Sandbox toggle in the new-session form

**Type:** Feature

`CreateSessionPane` gains a `sandbox` checkbox (`data-testid="create-session-sandbox-toggle"`) in the Claude CLI fields; submit sends `StartSessionRequest.sandbox`. Regenerated `connection_pb.ts` (adds the `sandbox` field). Component test `CreateSessionSandboxToggle.cy.tsx` (in-memory backend, asserts the typed request via `callsTo`). (tddy-web)

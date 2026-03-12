# PRD: Session Lifecycle Redesign

## Summary

Redesign how agent sessions are created, persisted, and resumed across the TDD workflow. Remove hardcoded `is_resume` hacks from per-step hooks, centralize session tracking in the changeset `state` section, create changesets earlier (before workflow starts), and capture session IDs from the first agent stream event.

## Background

The current session management has several problems:

1. **`is_resume` is hardcoded per-step** -- `before_acceptance_tests` and `before_green` explicitly set `context.set_sync("is_resume", true)` and look up session IDs via tag-based queries (`get_session_for_tag(&changeset, "plan")`). This is fragile and led to a crash: acceptance-tests tried to `--resume` a plan-mode session that Claude CLI couldn't find.

2. **Changeset created too late** -- `changeset.yaml` is only written in `after_plan` (after the entire plan agent finishes). If the plan step fails or is interrupted, there's no changeset on disk, making recovery impossible.

3. **Session data written too late** -- Session entries are only persisted to changeset after the entire step completes. If the process crashes mid-step, the session_id is lost.

4. **No active session tracking** -- The `state` section has `current` (workflow state) and `history`, but no concept of which agent session is currently active. Steps use ad-hoc tag lookups to find sessions to resume.

## Affected Features

- [planning-step.md](../planning-step.md) -- session creation for plan goal
- [implementation-step.md](../implementation-step.md) -- session creation/resume for acceptance-tests, red, green goals

## Requirements

### R1: Active session in state

Add a `session_id` field to the `state` section of `changeset.yaml`. Every workflow step updates `state.session_id` when it starts running. This is the single source of truth for the currently-active agent session.

```yaml
state:
  current: Planning
  session_id: 019ce2c2-e1e1-7141-a2e5-e0165407b553
  updated_at: "2026-03-12T16:00:31Z"
  history: [...]
```

### R2: Early changeset creation

Create `changeset.yaml` immediately after the user enters their first prompt, before the workflow starts. Applies to all entry paths (TUI, CLI/plain, daemon). The initial changeset contains `initial_prompt`, `state.current = "Init"`, empty `sessions`, and default models.

### R3: Session capture from first stream event

When the first `system/init` event arrives from the Claude CLI stream (containing `session_id`), immediately:
1. Create a session entry in `changeset.sessions`
2. Update `state.session_id` to point to this session
3. Write changeset to disk

This ensures session data is persisted as early as possible, not after the step finishes.

### R4: Remove is_resume hack

Remove explicit `context.set_sync("is_resume", true)` from per-step hooks. Each step's `before_*` hook still decides whether to create a fresh session or resume an existing one, but reads `state.session_id` from changeset rather than hardcoding tag lookups. The `is_resume` decision is derived from state, not forced.

### R5: Per-step resume decision preserved

Each before-hook retains its own logic for deciding fresh vs resume:
- **acceptance-tests**: decides whether to resume the plan session or create fresh (currently broken for plan-mode sessions)
- **red**: creates a fresh session (current behavior)
- **green**: resumes the impl session from red
- **later steps**: each makes its own decision based on state

But all hooks read from `state.session_id` instead of `get_session_for_tag` with hardcoded tags.

## Success Criteria

1. Changeset exists on disk before any agent invocation
2. Session ID appears in changeset within seconds of agent starting (not after step completes)
3. No `context.set_sync("is_resume", true)` in hook code
4. `state.session_id` always reflects the currently-running agent session
5. Acceptance-tests step no longer crashes trying to resume plan-mode sessions
6. All existing tests pass
7. Session resume works correctly for steps that share a conversation thread (red → green)

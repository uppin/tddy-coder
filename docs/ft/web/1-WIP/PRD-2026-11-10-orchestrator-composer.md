# Start an orchestrator from the project entry — PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#orchestrators` 8/11 (`feature/orchestrators/orchestrator-composer`)

## Affected Features

- **Primary**: [Session drawer § Project Groups](../session-drawer.md#project-groups) — a "+" on the
  project header
- [Session drawer § Create Session](../session-drawer.md#create-session) — a composer variant
- [Project concept § Project-home session](../../daemon/project-concept.md#project-home-session)

## Summary

Each project group header in the session drawer gets a **"+"** button. It opens the **orchestrator
composer** in the detail area at `#/sessions/project/<id>/new`: an empty screen with a heading and a
prompt box **centred vertically**, and under the box only these controls — **model**, **effort**,
**provider account**, **host** and an **attachments** button. Sending (Enter, or the send button)
starts an orchestrator session for that project on the chosen host with the text as its initial
prompt and the attachments attached, then selects and attaches it as a created session is. A refusal is
shown under the box with the daemon's reason; the text is kept.

## Proposed Changes

### What's Changing
- The project header becomes a row: the collapse toggle, then "+" (`sessions-drawer-project-new-<id>`).
- Every project the host lists gets a group, even one with no main and no session, so "+" is reachable.
- `OrchestratorComposer` in `tddy-session-create-web`, reusing the form's model, effort, provider,
  host and attachment pieces; the host picker is split out of `CreateSessionHostAndProjectFields`.
- Route `#/sessions/project/<id>/new`, accepted by `isSessionsDrawerPath`.
- The start request: `session_role: orchestrator`, `session_type` = the first agent whose spec declares
  `project_home_main`, `project_id`, `initial_prompt`, `model`, `effort`, `provider_account_id`,
  `daemon_instance_id`, `attachments`.

### What's Staying the Same
- The pinned main entry and its setup pane; the global New session button and form.

## Acceptance Criteria
- [ ] "+" on a project header opens the composer with the box vertically centred
- [ ] The composer shows exactly model, effort, provider account, host, attachments
- [ ] Sending starts an orchestrator with the text as initial prompt and selects it
- [ ] A refusal keeps the text and shows the reason
- [ ] A project with no sessions and no main still shows its header and "+"

## References
- [Session drawer](../session-drawer.md), [Project concept](../../daemon/project-concept.md)

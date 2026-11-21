# 2026-11-21 — An orchestrator asks the operator for read or write access to named paths

**Type:** Feature · **Stack:** `#orchestra` 6/9 (PR #984) · **Packages:** `tddy-tool-engine`, `tddy-session-activity`, `tddy-daemon-rpc`, `tddy-session-lifecycle`, `tddy-service`, `tddy-tools`, `tddy-session-web` ·
Feature: [project-concept.md § Access requests](../../ft/daemon/project-concept.md#access-requests)

## Problem

An orchestrator was read-only and confined to its reach (`#orchestra` 1/9). It could not ask for more,
the operator had nowhere to answer, and nothing wrote the session's grant store.

## What changed

- **`tddy-tool-engine::path_access_tools`** (new): `RequestPathAccess { paths, mode, reason }`, the
  `PathAccessPort` the daemon implements (`paths_outside_reach`, `request_path_access`, both required)
  and `execute_path_access_tool`. A relative path, an empty list or an empty reason is refused before
  anything is asked. Paths already usable at the mode are not put to the operator; the answer is
  `{ answer: allowed | denied | unanswered | in_reach, paths, in_reach }`. Offered to orchestrator roles
  by the catalogue, the read-only allow list and `tddy-tools`' remote admission.
- **`tddy-session-activity::path_access`** (new): `PathAccessRequests`, the host's in-memory requests —
  record, list per session, answer (Allow writes a `PathGrant` per path through n1's `PathGrantStore`),
  withdraw, and a bounded wait (`PATH_ACCESS_WAIT`, 10 min) that ends unanswered.
- **`activity.proto`**: `ListPathAccessRequests` / `AnswerPathAccess` and `PathAccessRequest`, served by
  `ActivityServiceImpl` and delegated locally (never routed) by `PeerRoutedActivity`. TS regenerated.
- **`tddy-daemon-rpc`**: `exec_tool/path_access.rs` serves the tool for an orchestrator caller: the
  reach check, the request, a `path_access` pending-question notification on the bus, the wait.
  `orchestrator_reach.rs` gained `orchestrator_read_reach` (extracted) and `run_granted_write` /
  `granted_write`; `role_guard.rs` admits an orchestrator's own `Write` / `StrReplace` / `Delete` inside
  a write grant and nothing else.
- **`tddy-session-web`**: `PathAccessRequests`, mounted by `SessionWithActivitySidebar` for orchestrator
  roles, lists the requests with Allow / Deny.

## Decisions

- **The notification is a pending question of kind `path_access`**, so the drawer, desktop and
  Telegram announce it with no new source or kind.
- **Reach first** (validation): a path in the read reach, or inside a write grant for `write`, is
  answered `in_reach` without asking; a mixed request asks only for the rest. A read grant does not
  answer a write request.
- **The pane polls every 3 s** rather than reacting to the `path_access` notification: the
  notification feed is the drawer's one daemon-wide stream (NFR1), and nothing is announced when a
  request is answered elsewhere or times out, so only a re-read sees it leave.
- **An answer that fails** for any reason but not-found keeps the request in the pane and shows why;
  not-found (taken already) removes it.
- **`orchestrator_caller` stays in `base_freshness.rs`, `pub(super)`.** It is the role-generic
  orchestrator check, so the dependency is sound; moving it to `project_home/caller.rs` is the better
  home, but `#orchestra` 7/9 and 8/9 already import it from `base_freshness`, so the move belongs to
  the stack's sweep, not a per-node wrap.
- Session-scoped grants; both orchestrator roles; requests held by the daemon running the
  orchestrator (developer, 2026-11-21).
- Validation cleanups: the port's refusing default removed; mode naming in one place (`mode_name`);
  `Waiting::grant`, `PendingPathAccess::into_proto`, `DaemonPathAccess::announce` and
  `is_orchestrator_session` extracted.

## Tests

`packages/tddy-daemon-rpc/tests/path_access_elicitation_acceptance.rs` (6, real `ExecuteTool` and
`ActivityService`, notifications recorded), unit tests in `tddy-session-activity::path_access` (6) and
`tddy-tool-engine::path_access_tools` (6), and
`packages/tddy-session-web/cypress/component/PathAccessRequestsAcceptance.cy.tsx` (6).

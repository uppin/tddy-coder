# 2026-11-21 — An orchestrator writes session files and attaches them to the sessions it starts

**Type:** Feature · **Stack:** `#orchestra` 3/9 (PR #981) · **Packages:** `tddy-session-files`, `tddy-tool-engine`, `tddy-daemon-rpc`, `tddy-tools` ·
Feature: [session-attachments.md § Session files an orchestrator writes](../../ft/coder/session-attachments.md#session-files-an-orchestrator-writes), [project-concept.md](../../ft/daemon/project-concept.md)

## Problem

`StartSession` took attachments by reference to existing documents, but nothing let an agent create
one, and an orchestrator could not write at all.

## What changed

- **`tddy-session-files::session_files`** (new): `SessionFileStore::{open, write, list}` over
  `<session_dir>/artifacts/files/<name>`; flat-name validation, 1 MiB cap, atomic write via a hidden
  temporary that `list` skips.
- **`tddy-tool-engine::session_file_tools`** (new): the two tool definitions, `SessionFilePort`,
  `execute_session_file_tool`, and `attachment_for` — the `SESSION_ARTIFACT` `attachments[]` entry.
  Offered to `project-home` roles by the catalogue, the read-only allow list and `tddy-tools`' remote
  admission.
- **`tddy-daemon-rpc::exec_tool::session_files`** (new): `DaemonSessionFiles` port over the caller's
  own session directory; the role check reads the caller's `.session.yaml` and refuses a non-orchestrator.

## Decisions

- **No new `StartSession` attachment source.** A session file lives under the orchestrator's
  `SESSION_ARTIFACT` scope, which `StartSession` already attaches by reference; `start_session_args`
  is untouched, so the `schema_offering` complexity record is not grown.
- **Both orchestrator roles** get the tools; they share `is_orchestrator_role`.
- **Peer forwarding is out of scope**: the changeset's responsibility is the tool and the attachment
  answer. Recorded in `docs/dev/todo/2026-11-21-session-file-tools-are-not-forwarded-to-a-peer-host.md`.
- Validation cleanup: `SessionFilePort`'s methods are required (the refusing defaults were removed) and
  `entry_json` lost an unused parameter.

## Tests

`packages/tddy-daemon-rpc/tests/orchestrator_session_files_acceptance.rs` (5, through the real
`ExecuteTool`), the store's 5 unit tests and the tool module's 3.

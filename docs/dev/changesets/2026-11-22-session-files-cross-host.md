# 2026-11-22 — An orchestrator's session files reach the sessions it starts on other hosts

**Type:** Feature · **Stack:** `#orchestra` 10/11 (PR #996) · **Packages:** `tddy-tool-engine`, `tddy-daemon-rpc` ·
Feature: [session-attachments.md § Session files an orchestrator writes](../../ft/coder/session-attachments.md#session-files-an-orchestrator-writes)

## Problem

The session-file attachment carried no `daemon_instance_id`, so a session started on another host looked for the file in its own sessions base; and the two session-file tools were served only on the orchestrator's own host.

## What changed

- **`tddy-tool-engine::session_file_tools`**: `attachment_for(caller, holding_daemon, name)` names the daemon; `SessionFilePort::holding_daemon()`.
- **`tddy-daemon-rpc`**: `DaemonSessionFiles` answers its daemon's instance id; the session-file tools join the caller-held-elsewhere route (`exec_tool/project_home_remote.rs`), forwarded to the owner after it confirms the caller.
- Tests: `session_files_cross_host_acceptance.rs` (two in-memory daemons); the n3 attachment test gained `daemon_instance_id`.

## Not changed

Store, names, sizes and the worker refusal; the base-freshness tools stay unforwarded. The todo `session-file-tools-are-not-forwarded-to-a-peer-host` is closed.

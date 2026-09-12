#!/usr/bin/env python3
"""Rewrite `use tddy_service::proto::connection::{...}` into split modules."""

from __future__ import annotations

import re
import sys
from collections import defaultdict
from pathlib import Path

# Same map as migrate_proto_connection.py
TYPE_TO_MOD: dict[str, str] = {
    "AgentClonePlacement": "session",
    "AttachmentMaterializationProgress": "session",
    "BranchConflict": "session",
    "ConnectSessionRequest": "session",
    "ConnectSessionResponse": "session",
    "DeleteSessionRequest": "session",
    "DeleteSessionResponse": "session",
    "GetWorktreeSnapshotRequest": "session",
    "GetWorktreeSnapshotResponse": "session",
    "HostDocumentRef": "session",
    "ListSessionsRequest": "session",
    "ListSessionsResponse": "session",
    "ResumeSessionRequest": "session",
    "ResumeSessionResponse": "session",
    "SessionAttachment": "session",
    "SessionContextDoc": "session",
    "SessionContextDocKind": "session",
    "SessionEntry": "session",
    "Signal": "session",
    "SignalSessionRequest": "session",
    "SignalSessionResponse": "session",
    "SplitAgentPlacement": "session",
    "StagedAttachmentRef": "session",
    "StartSessionEvent": "session",
    "StartSessionRequest": "session",
    "StartSessionResponse": "session",
    "AddProjectToHostRequest": "project",
    "AddProjectToHostResponse": "project",
    "CreateProjectRequest": "project",
    "CreateProjectResponse": "project",
    "ListProjectBranchesRequest": "project",
    "ListProjectBranchesResponse": "project",
    "ListProjectsRequest": "project",
    "ListProjectsResponse": "project",
    "ProjectEntry": "project",
    "SetProjectDefaultBranchRequest": "project",
    "SetProjectDefaultBranchResponse": "project",
    "DemoVmState": "demo_vm",
    "GetDemoVmStatusRequest": "demo_vm",
    "GetDemoVmStatusResponse": "demo_vm",
    "StartDemoVmRequest": "demo_vm",
    "StartDemoVmResponse": "demo_vm",
    "StopDemoVmRequest": "demo_vm",
    "StopDemoVmResponse": "demo_vm",
    "ExecuteToolChunk": "exec_tools",
    "ExecuteToolRequest": "exec_tools",
    "ExecuteToolResponse": "exec_tools",
    "ListExecToolsRequest": "exec_tools",
    "ListExecToolsResponse": "exec_tools",
    "ListSessionToolCallsRequest": "exec_tools",
    "ListSessionToolCallsResponse": "exec_tools",
    "ToolCallInfo": "exec_tools",
    "ToolDef": "exec_tools",
    "ListToolsRequest": "catalog",
    "ListToolsResponse": "catalog",
    "MintLocalTokenRequest": "local_token",
    "MintLocalTokenResponse": "local_token",
    "HostDocumentScope": "types",
    "SessionAgentActivity": "types",
    "SessionAgentStatus": "types",
    "BranchSession": "types",
    "BranchBaseSync": "pr_stack",
    "BranchWorktree": "pr_stack",
    "PrStatusView": "pr_stack",
    "ConnectionService": "session",
    "SessionService": "session",
    "ProjectService": "project",
    "DemoVmService": "demo_vm",
    "LocalTokenService": "local_token",
    "ExecToolService": "exec_tools",
    "ProbeOutcome": "host",  # was never in connection; leave for restructure tests
}

SUB_PATHS = {
    "session_attachment": "session",
    "start_session_event": "session",
}


def mod_for_item(item: str) -> str | None:
    item = item.strip()
    if not item:
        return None
    if item.startswith("session_attachment::") or item.startswith("start_session_event::"):
        return SUB_PATHS[item.split("::")[0]]
    base = item.split(" as ")[0].strip()
    base = base.split("::")[-1]
    if base == "ConnectionService":
        return "session"
    return TYPE_TO_MOD.get(base)


def remap_item(item: str) -> str:
    item = item.strip()
    if " as " in item:
        typ, alias = item.split(" as ", 1)
        typ = typ.strip()
        alias = alias.strip()
        if typ == "ConnectionService":
            typ = "SessionService"
            if alias == "ConnectionServiceTrait":
                alias = "SessionServiceTrait"
        return f"{typ} as {alias}"
    if item.startswith("session_attachment::") or item.startswith("start_session_event::"):
        return item
    if item == "ConnectionService":
        return "SessionService"
    return item


USE_RE = re.compile(
    r"use tddy_service::proto::connection::\{([^}]*)\};",
    re.MULTILINE | re.DOTALL,
)


def rewrite_uses(text: str) -> str:
    def repl(m: re.Match[str]) -> str:
        body = m.group(1)
        items = [x.strip() for x in body.split(",") if x.strip()]
        by_mod: dict[str, list[str]] = defaultdict(list)
        unknown: list[str] = []
        for item in items:
            mod = mod_for_item(item)
            if mod is None:
                unknown.append(item)
            else:
                by_mod[mod].append(remap_item(item))
        if unknown:
            return m.group(0)
        lines = []
        for mod in sorted(by_mod.keys()):
            joined = ", ".join(by_mod[mod])
            lines.append(f"use tddy_service::proto::{mod}::{{{joined}}};")
        return "\n".join(lines)

    return USE_RE.sub(repl, text)


SINGLE_USE_RE = re.compile(
    r"use tddy_service::proto::connection::([A-Za-z0-9_:]+(?:\s+as\s+[A-Za-z0-9_]+)?);"
)


def rewrite_single_uses(text: str) -> str:
    def repl(m: re.Match[str]) -> str:
        item = m.group(1).strip()
        mod = mod_for_item(item)
        if mod is None:
            return m.group(0)
        return f"use tddy_service::proto::{mod}::{remap_item(item)};"

    return SINGLE_USE_RE.sub(repl, text)


def main() -> int:
    root = Path(__file__).resolve().parents[1] / "packages"
    changed = 0
    for path in sorted(root.rglob("*.rs")):
        raw = path.read_text()
        if "use tddy_service::proto::connection::" not in raw:
            continue
        new = rewrite_single_uses(rewrite_uses(raw))
        if new != raw:
            path.write_text(new)
            changed += 1
            print(path)
    print(f"updated {changed} files", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

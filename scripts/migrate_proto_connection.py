#!/usr/bin/env python3
"""One-off: map tddy_service::proto::connection types to split proto modules."""

from __future__ import annotations

import re
import sys
from pathlib import Path

TYPE_TO_MOD = {
    # session
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
    # project
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
    # demo_vm
    "DemoVmState": "demo_vm",
    "GetDemoVmStatusRequest": "demo_vm",
    "GetDemoVmStatusResponse": "demo_vm",
    "StartDemoVmRequest": "demo_vm",
    "StartDemoVmResponse": "demo_vm",
    "StopDemoVmRequest": "demo_vm",
    "StopDemoVmResponse": "demo_vm",
    # exec_tools
    "ExecuteToolChunk": "exec_tools",
    "ExecuteToolRequest": "exec_tools",
    "ExecuteToolResponse": "exec_tools",
    "ListExecToolsRequest": "exec_tools",
    "ListExecToolsResponse": "exec_tools",
    "ListSessionToolCallsRequest": "exec_tools",
    "ListSessionToolCallsResponse": "exec_tools",
    "ToolCallInfo": "exec_tools",
    "ToolDef": "exec_tools",
    # catalog
    "ListToolsRequest": "catalog",
    "ListToolsResponse": "catalog",
    # local_token
    "MintLocalTokenRequest": "local_token",
    "MintLocalTokenResponse": "local_token",
    # types
    "HostDocumentScope": "types",
    "SessionAgentActivity": "types",
    "SessionAgentStatus": "types",
    "BranchSession": "types",
    # pr_stack
    "BranchBaseSync": "pr_stack",
    "BranchWorktree": "pr_stack",
    "PrStatusView": "pr_stack",
    # services
    "ConnectionService": "session",  # remapped to SessionService below
    "SessionService": "session",
    "ProjectService": "project",
    "DemoVmService": "demo_vm",
    "LocalTokenService": "local_token",
    "ExecToolService": "exec_tools",
}

SUBMODULE_PREFIXES = {
    "session_attachment": "session",
    "start_session_event": "session",
}


def remap_service_name(name: str, alias: str | None) -> str:
    if name == "ConnectionService":
        name = "SessionService"
        if alias == "ConnectionServiceTrait":
            alias = "SessionServiceTrait"
    out = name
    if alias:
        out = f"{name} as {alias}"
    return out


def migrate_text(text: str) -> str:
    # Qualified paths: tddy_service::proto::connection::Type
    def repl_qualified(m: re.Match[str]) -> str:
        rest = m.group(1)
        if rest.startswith("session_attachment::") or rest.startswith("start_session_event::"):
            sub = rest.split("::", 1)[0]
            mod = SUBMODULE_PREFIXES[sub]
            return f"tddy_service::proto::{mod}::{rest}"
        typ = rest.split("::")[0]
        if typ in TYPE_TO_MOD:
            mod = TYPE_TO_MOD[typ]
            return f"tddy_service::proto::{mod}::{rest}"
        return m.group(0)

    text = re.sub(
        r"tddy_service::proto::connection::([A-Za-z0-9_:]+)",
        repl_qualified,
        text,
    )

    # proto::connection:: in comments/strings - only fix code paths; leave string literals about connection for tests
    text = re.sub(
        r"(?<![\"'])proto::connection::([A-Za-z0-9_:]+)",
        lambda m: (
            f"proto::{SUBMODULE_PREFIXES[m.group(1).split('::')[0]]}::{m.group(1)}"
            if m.group(1).split("::")[0] in SUBMODULE_PREFIXES
            else (
                f"proto::{TYPE_TO_MOD[m.group(1).split('::')[0]]}::{m.group(1)}"
                if m.group(1).split("::")[0] in TYPE_TO_MOD
                else m.group(0)
            )
        ),
        text,
    )

    return text


def main() -> int:
    root = Path(__file__).resolve().parents[1] / "packages"
    changed = 0
    for path in sorted(root.rglob("*.rs")):
        raw = path.read_text()
        if "proto::connection" not in raw and "tddy_service::proto::connection" not in raw:
            continue
        new = migrate_text(raw)
        if new != raw:
            path.write_text(new)
            changed += 1
            print(path)
    print(f"updated {changed} files", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

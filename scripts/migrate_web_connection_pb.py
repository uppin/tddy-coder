#!/usr/bin/env python3
"""Migrate packages/tddy-web from connection_pb to split session/project/… protos."""

from __future__ import annotations

import re
import sys
from pathlib import Path

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
    "ListAgentsRequest": "catalog",
    "ListAgentsResponse": "catalog",
    "AgentInfo": "catalog",
    "ToolInfo": "catalog",
    "MintLocalTokenRequest": "local_token",
    "MintLocalTokenResponse": "local_token",
    "HostDocumentScope": "types",
    "SessionAgentActivity": "types",
    "SessionAgentStatus": "types",
    "BranchSession": "types",
    "BranchBaseSync": "pr_stack",
    "BranchRemote": "pr_stack",
    "BranchResolution": "pr_stack",
    "BranchWorktree": "pr_stack",
    "PrStatusView": "pr_stack",
    "ConnectionService": "session",
    "SessionService": "session",
    "ProjectService": "project",
    "DemoVmService": "demo_vm",
    "LocalTokenService": "local_token",
}

MOD_TO_PB = {
    "session": "session_pb",
    "project": "project_pb",
    "demo_vm": "demo_vm_pb",
    "local_token": "local_token_pb",
    "exec_tools": "exec_tools_pb",
    "catalog": "catalog_pb",
    "types": "types_pb",
    "pr_stack": "pr_stack_pb",
}

PROJECT_METHODS = {
    "listProjects",
    "createProject",
    "addProjectToHost",
    "listProjectBranches",
    "setProjectDefaultBranch",
}
DEMO_METHODS = {"startDemoVm", "stopDemoVm", "getDemoVmStatus"}

RPC_PATH_BY_METHOD: dict[str, str] = {
    "ListSessions": "session.SessionService/ListSessions",
    "StartSession": "session.SessionService/StartSession",
    "StreamStartSession": "session.SessionService/StreamStartSession",
    "ConnectSession": "session.SessionService/ConnectSession",
    "ResumeSession": "session.SessionService/ResumeSession",
    "SignalSession": "session.SessionService/SignalSession",
    "DeleteSession": "session.SessionService/DeleteSession",
    "GetWorktreeSnapshot": "session.SessionService/GetWorktreeSnapshot",
    "ListProjects": "project.ProjectService/ListProjects",
    "CreateProject": "project.ProjectService/CreateProject",
    "AddProjectToHost": "project.ProjectService/AddProjectToHost",
    "ListProjectBranches": "project.ProjectService/ListProjectBranches",
    "SetProjectDefaultBranch": "project.ProjectService/SetProjectDefaultBranch",
    "StartDemoVm": "demo_vm.DemoVmService/StartDemoVm",
    "StopDemoVm": "demo_vm.DemoVmService/StopDemoVm",
    "GetDemoVmStatus": "demo_vm.DemoVmService/GetDemoVmStatus",
    "MintLocalToken": "local_token.LocalTokenService/MintLocalToken",
}

IMPORT_RE = re.compile(
    r"""import\s*\{([^}]+)\}\s*from\s*(['"])([^'"]*connection_pb)(['"])\s*;""",
    re.MULTILINE,
)


def mod_for_symbol(sym: str) -> str | None:
    sym = sym.strip()
    if sym.startswith("type "):
        sym = sym[5:].strip()
    base = sym.split(" as ")[0].strip()
    return TYPE_TO_MOD.get(base)


def rewrite_imports(text: str) -> str:
    def repl(m: re.Match[str]) -> str:
        body = m.group(1)
        quote = m.group(2)
        path = m.group(3)
        parts = [p.strip() for p in body.split(",") if p.strip()]
        by_mod: dict[str, list[str]] = {}
        for part in parts:
            mod = mod_for_symbol(part)
            if mod is None:
                mod = "session"
            by_mod.setdefault(mod, []).append(part)
        if not by_mod:
            return m.group(0)
        dir_path = path.rsplit("/", 1)[0] if "/" in path else "."
        lines: list[str] = []
        for mod in sorted(by_mod.keys(), key=lambda x: MOD_TO_PB[x]):
            pb = MOD_TO_PB[mod]
            new_path = f"{dir_path}/{pb}" if dir_path else pb
            items = ", ".join(by_mod[mod])
            lines.append(f"import {{ {items} }} from {quote}{new_path}{quote};")
        return "\n".join(lines)

    return IMPORT_RE.sub(repl, text)


def service_for_file(text: str) -> str:
    if any(m in text for m in DEMO_METHODS):
        return "demo_vm"
    if any(m in text for m in PROJECT_METHODS) and not any(
        x in text
        for x in (
            "listSessions",
            "startSession",
            "connectSession",
            "resumeSession",
            "signalSession",
            "deleteSession",
            "getWorktreeSnapshot",
            "streamStartSession",
        )
    ):
        return "project"
    return "session"


def rewrite_services(text: str) -> str:
    uses_project = any(m in text for m in PROJECT_METHODS)
    uses_session = any(
        m in text
        for m in (
            "listSessions",
            "startSession",
            "connectSession",
            "resumeSession",
            "signalSession",
            "deleteSession",
            "getWorktreeSnapshot",
            "streamStartSession",
        )
    )
    uses_demo = any(m in text for m in DEMO_METHODS)

    if uses_demo:
        text = text.replace("ConnectionService", "DemoVmService")
        text = text.replace("DemoVmService", "DemoVmService", 1)  # noop anchor
    elif uses_project and not uses_session:
        text = text.replace("ConnectionService", "ProjectService")
    else:
        text = text.replace("ConnectionService", "SessionService")

    if uses_project and uses_session:
        text = text.replace("useDaemonClient(SessionService)", "useDaemonClient(SessionService)")
        if "useDaemonClient(ProjectService)" not in text:
            text = text.replace(
                "const client = useDaemonClient(SessionService);",
                "const client = useDaemonClient(SessionService);\n"
                "  const projectClient = useDaemonClient(ProjectService);",
                1,
            )
        text = re.sub(
            r"(\w+)\?\.(listProjects|createProject|addProjectToHost|listProjectBranches|setProjectDefaultBranch)\(",
            lambda m: (
                f"projectClient?.{m.group(2)}("
                if m.group(1) == "client"
                else f"{m.group(1)}?.{m.group(2)}("
            ),
            text,
        )
        text = re.sub(
            r"(\w+)\.(listProjects|createProject|addProjectToHost|listProjectBranches|setProjectDefaultBranch)\(",
            lambda m: (
                f"projectClient.{m.group(2)}("
                if m.group(1) == "client"
                else f"{m.group(1)}.{m.group(2)}("
            ),
            text,
        )
        if "ProjectService" not in text and "project_pb" in text:
            pass
        elif uses_project and "ProjectService" not in text:
            text = text.replace(
                'from "../../gen/session_pb"',
                'from "../../gen/session_pb"\nimport { ProjectService } from "../../gen/project_pb"',
                1,
            )

    return text


def rewrite_rpc_urls(text: str) -> str:
    text = text.replace(
        "the pre-unbundle monolithic RPC coordinate",
        "session.SessionService",
    )
    text = text.replace("connection.ConnectionService", "session.SessionService")
    for method, path in RPC_PATH_BY_METHOD.items():
        text = text.replace(f"**/rpc/session.SessionService/{method}", f"**/rpc/{path}")
    # project intercepts that still say session.SessionService
    for method in (
        "ListProjects",
        "CreateProject",
        "AddProjectToHost",
        "ListProjectBranches",
        "SetProjectDefaultBranch",
    ):
        path = RPC_PATH_BY_METHOD[method]
        text = text.replace(f"**/rpc/session.SessionService/{method}", f"**/rpc/{path}")
    return text


def migrate_file(path: Path, raw: str) -> str:
    if "connection_pb" not in raw and "ConnectionService" not in raw and "the pre-unbundle" not in raw:
        return raw
    text = rewrite_rpc_urls(raw)
    text = rewrite_imports(text)
    text = rewrite_services(text)
    text = text.replace("connection_pb", "session_pb")
    return text


def main() -> int:
    root = Path(__file__).resolve().parents[1] / "packages" / "tddy-web"
    changed = 0
    for path in sorted(root.rglob("*")):
        if path.suffix not in {".ts", ".tsx"}:
            continue
        if "/gen/" in str(path) or "/docs/" in str(path):
            continue
        raw = path.read_text()
        new = migrate_file(path, raw)
        if new != raw:
            path.write_text(new)
            changed += 1
            print(path)
    print(f"updated {changed} files", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

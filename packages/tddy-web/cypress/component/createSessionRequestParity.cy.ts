/**
 * Whatever the New-session form can send, an orchestrator agent can send too (`#start-parity` 2/9).
 *
 * `buildStartSessionRequest` is the one place the form turns its state into a `StartSession`
 * request. This spec runs it for every session type with every option set — split and co-located
 * codebases, a stack parent, the pr-stack recipe, a host-document attachment — and requires each
 * request key it emits to be one the `StartSession` exec tool offers, or one the tool fills from
 * its caller's context. The tool's side of the list is the fixture `cargo test -p tddy-tool-engine`
 * keeps current (`tests/start_session_args_parity.rs`), so a form option added without the tool
 * fails here.
 *
 * Pure-function spec: nothing is mounted.
 *
 * Changeset: docs/dev/1-WIP/2026-10-25-start-parity-start-tool.md
 */

import type { SessionAttachmentInit } from "../../src/hooks/useSessionAttachments";
import {
  buildStartSessionRequest,
  type CreateSessionFormValues,
  type SessionType,
} from "../../src/components/sessions/createSessionRequest";
import toolFields from "../support/fixtures/start-session-tool-fields.json";

const SESSION_TYPES: SessionType[] = [
  "tool",
  "claude-cli",
  "cursor-cli",
  "opencode-cli",
  "muse-cli",
  "warp-cli",
];

/** A form with every value the pane can hold set to something non-empty. */
function aFormWithEveryOptionSet(
  sessionType: SessionType,
  values: Partial<CreateSessionFormValues>,
): CreateSessionFormValues {
  return {
    sessionToken: "token-1",
    projectId: "project-1",
    branchIntent: "new_branch_from_base",
    newBranchName: "feature/x",
    createRemoteBranch: true,
    selectedBaseBranch: "origin/main",
    selectedBranchToWorkOn: "feature/y",
    daemonInstanceId: "host-a",
    sessionType,
    toolPath: "/usr/bin/tddy-coder",
    selectedAgentId: "claude",
    recipe: "pr-stack",
    stackParent: "parent-session",
    stackNodeId: "node-1",
    stackParentDaemonInstanceId: "host-a",
    prStackBaseSessionId: "base-session",
    model: "sonnet",
    providerAccountId: "account-1",
    providerBuiltIn: false,
    permissionMode: "plan",
    dangerouslySkipPermissions: true,
    placementWithdrawsPermissionBypass: false,
    initialPrompt: "do the thing",
    sandbox: true,
    managedCodebase: true,
    sandboxedCodebase: false,
    isSplitCodebase: false,
    selectedAgentIds: ["fastcontext@host-a"],
    clonePlacedAgentIds: ["fastcontext@host-a"],
    semanticIndex: true,
    codebaseDaemonInstanceId: "",
    sshConfigHost: "builder",
    ...values,
  };
}

const A_HOST_DOCUMENT: SessionAttachmentInit = {
  basename: "brief.md",
  source: {
    case: "hostDocument",
    value: { daemonInstanceId: "host-a", relativePath: "docs/brief.md" },
  },
} as SessionAttachmentInit;

/** The request keys the form sends, over every session type and both codebase placements. */
function everyKeyTheFormSends(): Set<string> {
  const keys = new Set<string>();
  for (const sessionType of SESSION_TYPES) {
    for (const placement of [
      { isSplitCodebase: false, sandboxedCodebase: false, codebaseDaemonInstanceId: "" },
      { isSplitCodebase: true, sandboxedCodebase: false, codebaseDaemonInstanceId: "host-b" },
      { isSplitCodebase: false, sandboxedCodebase: true, sandbox: false, managedCodebase: false },
    ]) {
      const request = buildStartSessionRequest(
        aFormWithEveryOptionSet(sessionType, placement),
        null,
        [A_HOST_DOCUMENT],
      );
      Object.keys(request).forEach((key) => keys.add(key));
    }
  }
  return keys;
}

const snakeCase = (camel: string): string => camel.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);

describe("StartSession tool parity with the New-session form", () => {
  it("offers every request field the New-session form can send", () => {
    // Given what the tool offers or fills from its caller
    const accepted = new Set<string>([...toolFields.offered, ...toolFields.context_filled]);

    // When every key the form can send is collected
    const sent = [...everyKeyTheFormSends()].map(snakeCase);

    // Then the tool accepts each of them
    const missing = sent.filter((field) => !accepted.has(field)).sort();
    expect(missing, "form fields the StartSession tool does not offer").to.deep.equal([]);
  });

  it("is checked against a tool that offers something", () => {
    // Then the fixture is the tool's real list, not an empty one that would accept nothing
    expect(toolFields.offered).to.include("session_type");
    expect(toolFields.offered).to.include("daemon_instance_id");
  });
});

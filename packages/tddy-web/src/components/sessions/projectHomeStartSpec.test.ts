/**
 * Unit tests for the start spec the main variant of the New-session form sends: what the operator
 * chose goes out, and the fields the daemon owns for main do not.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md
 */

import { describe, expect, it } from "bun:test";
import { buildProjectHomeStartSpec, type CreateSessionFormValues } from "./createSessionRequest";

/** A filled-in Claude CLI form, branch and placement fields included as the pane holds them. */
function aClaudeCliForm(overrides: Partial<CreateSessionFormValues> = {}): CreateSessionFormValues {
  return {
    sessionToken: "tok",
    projectId: "proj-beta",
    branchIntent: "new_branch_from_base",
    newBranchName: "feature/x",
    createRemoteBranch: true,
    selectedBaseBranch: "origin/master",
    selectedBranchToWorkOn: "feature/y",
    daemonInstanceId: "host-1",
    sessionType: "claude-cli",
    toolPath: "",
    selectedAgentId: "",
    recipe: "tdd",
    stackParent: "parent-1",
    stackNodeId: "node-1",
    stackParentDaemonInstanceId: "host-1",
    prStackBaseSessionId: "",
    model: "sonnet",
    providerAccountId: "acct-meta",
    providerBuiltIn: false,
    permissionMode: "plan",
    dangerouslySkipPermissions: false,
    placementWithdrawsPermissionBypass: false,
    initialPrompt: "Watch the stack",
    sandbox: true,
    managedCodebase: false,
    sandboxedCodebase: true,
    isSplitCodebase: true,
    selectedAgentIds: ["reviewer"],
    semanticIndex: true,
    codebaseDaemonInstanceId: "host-2",
    sshConfigHost: "",
    ...overrides,
  };
}

describe("buildProjectHomeStartSpec", () => {
  it("carries the session type, model, provider, permission mode and initial prompt the operator chose", () => {
    const spec = buildProjectHomeStartSpec(aClaudeCliForm());

    expect(spec).toMatchObject({
      sessionType: "claude-cli",
      model: "sonnet",
      providerAccountId: "acct-meta",
      providerBuiltIn: false,
      permissionMode: "plan",
      dangerouslySkipPermissions: false,
      initialPrompt: "Watch the stack",
      specializedAgents: ["reviewer"],
      semanticIndex: true,
    });
  });

  it("carries no project, branch, stack or placement fields", () => {
    const spec = buildProjectHomeStartSpec(aClaudeCliForm());

    expect(Object.keys(spec).sort()).toEqual([
      "dangerouslySkipPermissions",
      "initialPrompt",
      "model",
      "permissionMode",
      "providerAccountId",
      "providerBuiltIn",
      "semanticIndex",
      "sessionType",
      "specializedAgents",
    ]);
  });
});

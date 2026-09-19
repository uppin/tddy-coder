/**
 * Unit tests for the create-session request builder.
 *
 * `buildStartSessionRequest` assembles the three per-session-type `StartSessionRequest` shapes the
 * new-session form submits. It was lifted out of `CreateSessionPane.tsx` as a pure function over a
 * named form-value bag, so which fields each session type sends — and, more importantly, which it
 * deliberately sends *empty* — can be asserted directly instead of only through the DOM.
 *
 * Run as a Cypress spec because that is this package's only working test runner — `tsc` is not a
 * gate here. Nothing is mounted: these are pure-function tests.
 *
 * Changeset: docs/dev/1-WIP/2026-09-18-sandboxed-codebase-mode-from-the-web.md
 */

import type { SessionAttachmentInit } from "../../src/hooks/useSessionAttachments";
import {
  buildStartSessionRequest,
  type CreateSessionFormValues,
} from "../../src/components/sessions/createSessionRequest";

/**
 * A form holding nothing but a token and a project — every other value at the empty state the pane
 * starts it in. Each test names the handful of values its own claim rests on, so what a case
 * depends on is what it spells out.
 */
function aFormWith(values: Partial<CreateSessionFormValues>): CreateSessionFormValues {
  return {
    sessionToken: "token-1",
    projectId: "project-1",
    branchIntent: "new_branch_from_base",
    newBranchName: "",
    createRemoteBranch: true,
    selectedBaseBranch: "",
    selectedBranchToWorkOn: "",
    daemonInstanceId: "",
    sessionType: "claude-cli",
    toolPath: "",
    selectedAgentId: "",
    recipe: "tdd",
    stackParent: "",
    stackNodeId: "",
    stackParentDaemonInstanceId: "",
    prStackBaseSessionId: "",
    model: "sonnet",
    permissionMode: "auto",
    dangerouslySkipPermissions: false,
    placementWithdrawsPermissionBypass: false,
    initialPrompt: "",
    sandbox: false,
    managedCodebase: false,
    sandboxedCodebase: false,
    isSplitCodebase: false,
    selectedAgentIds: [],
    semanticIndex: false,
    codebaseDaemonInstanceId: "",
    sshConfigHost: "",
    ...values,
  };
}

describe("create session request builder", () => {
  it("asks to be refused rather than renamed when another session owns the branch", () => {
    // Given any form at all — this form has an operator to prompt
    const form = aFormWith({});

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then the daemon is told to reject rather than silently hand back "<branch>-1"
    expect(request.onBranchConflict).to.equal("reject");
  });

  it("sends a tool session as the agent the operator picked, on the tool path", () => {
    // Given a tool session with an agent and a tool path
    const form = aFormWith({
      sessionType: "tool",
      selectedAgentId: "explorer",
      toolPath: "/usr/bin/tddy-coder",
    });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then the agent and tool path go out, and the session type is left empty — a tool session is
    // what the daemon starts when no session type names a CLI
    expect(request.agent).to.equal("explorer");
    expect(request.toolPath).to.equal("/usr/bin/tddy-coder");
    expect(request.sessionType).to.equal("");
  });

  it("names no owning host for a tool session's stack parent", () => {
    // Given a tool session parented to an orchestrator
    const form = aFormWith({ sessionType: "tool", stackParent: "session-7" });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then the host is left empty: a tool session's chain base is resolved by the tddy-coder the
    // daemon spawns, against that process's own sessions tree, so a host here names a routing the
    // start never performs
    expect(request.stackParent).to.equal("session-7");
    expect(request.stackParentDaemonInstanceId).to.equal("");
  });

  it("seeds a stack only for the recipe whose picker offered a base session", () => {
    // Given a base session chosen while the recipe was pr-stack
    const form = aFormWith({
      sessionType: "tool",
      recipe: "pr-stack",
      prStackBaseSessionId: "session-9",
    });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then the seed goes out with it
    expect(request.prStackBaseSessionId).to.equal("session-9");
  });

  it("drops a base session chosen before the recipe was switched away from pr-stack", () => {
    // Given a base session still held from a pr-stack recipe the operator has since left
    const form = aFormWith({
      sessionType: "tool",
      recipe: "tdd",
      prStackBaseSessionId: "session-9",
    });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then it does not leak into the request — the daemon refuses a base session beside any other
    // recipe, so a stale choice would turn a valid start into a refusal
    expect(request.prStackBaseSessionId).to.equal("");
  });

  it("sends no codebase host for a cursor-cli session", () => {
    // Given a cursor-cli session whose form still holds a host picked while it was claude-cli
    const form = aFormWith({
      sessionType: "cursor-cli",
      managedCodebase: true,
      codebaseDaemonInstanceId: "host-b",
      isSplitCodebase: true,
    });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then no host goes out: cursor-agent has no tool allowlist, so a split codebase could only be
    // suggested to it, never enforced, and the daemon refuses such a request
    expect(request.sessionType).to.equal("cursor-cli");
    expect(request.codebaseDaemonInstanceId).to.equal("");
  });

  it("withdraws the recipe from a split claude-cli session", () => {
    // Given a split placement — the worktree is on another daemon than the agent
    const form = aFormWith({
      sessionType: "claude-cli",
      managedCodebase: true,
      isSplitCodebase: true,
      codebaseDaemonInstanceId: "host-b",
      recipe: "tdd",
    });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then the recipe is dropped — its tooling runs against a repository on the agent's daemon,
    // which a split session does not have, and the daemon refuses the combination
    expect(request.recipe).to.equal("");
    expect(request.codebaseDaemonInstanceId).to.equal("host-b");
  });

  it("keeps the recipe on a co-located managed claude-cli session", () => {
    // Given a managed codebase on the session's own host
    const form = aFormWith({
      sessionType: "claude-cli",
      managedCodebase: true,
      isSplitCodebase: false,
      recipe: "bugfix",
    });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then the recipe goes out, and no codebase host is named
    expect(request.recipe).to.equal("bugfix");
    expect(request.codebaseDaemonInstanceId).to.equal("");
  });

  it("sends no permission bypass on a placement that withdraws it", () => {
    // Given the bypass still held in form state under a placement that withdraws its control
    const form = aFormWith({
      sessionType: "claude-cli",
      dangerouslySkipPermissions: true,
      placementWithdrawsPermissionBypass: true,
    });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then the flag is withheld — on these placements the agent's deny list *is* the confinement
    expect(request.dangerouslySkipPermissions).to.equal(false);
  });

  it("sends the permission bypass on a placement that still offers it", () => {
    // Given the bypass chosen on an ordinary co-located session
    const form = aFormWith({
      sessionType: "claude-cli",
      dangerouslySkipPermissions: true,
      placementWithdrawsPermissionBypass: false,
    });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then it goes out as chosen
    expect(request.dangerouslySkipPermissions).to.equal(true);
  });

  it("sends the jailed-codebase placement with the placements it replaced already cleared", () => {
    // Given the jailed codebase chosen, which cleared the other two in the form
    const form = aFormWith({
      sessionType: "claude-cli",
      sandboxedCodebase: true,
      sandbox: false,
      managedCodebase: false,
    });

    // When the request is built
    const request = buildStartSessionRequest(form, null, []);

    // Then the request says what the screen says — one placement, the other two empty
    expect(request.sandboxedCodebase).to.equal(true);
    expect(request.sandbox).to.equal(false);
    expect(request.managedCodebase).to.equal(false);
  });

  it("replaces the branch fields with a conflict resolution's own", () => {
    // Given a form holding the branch the daemon refused
    const form = aFormWith({ sessionType: "claude-cli", newBranchName: "feature/taken" });

    // When the operator resolves the conflict by renaming
    const request = buildStartSessionRequest(form, { newBranchName: "feature/taken-2" }, []);

    // Then the resolution's branch is what goes out
    expect(request.newBranchName).to.equal("feature/taken-2");
  });

  it("carries the attachments the form staged", () => {
    // Given one staged attachment
    const attachment: SessionAttachmentInit = {
      basename: "PRD.md",
      source: {
        case: "staged",
        value: { daemonInstanceId: "host-a", stagingId: "staged-1" },
      },
    };

    // When the request is built with it
    const request = buildStartSessionRequest(aFormWith({}), null, [attachment]);

    // Then it rides along
    expect(request.attachments).to.deep.equal([attachment]);
  });

  it("sends an empty attachment list for a form with nothing attached", () => {
    // Given a form with nothing attached
    // When the request is built
    const request = buildStartSessionRequest(aFormWith({}), null, []);

    // Then the list is empty — byte-for-byte the request this pane has always sent
    expect(request.attachments).to.deep.equal([]);
  });
});

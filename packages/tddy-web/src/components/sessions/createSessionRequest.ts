import type { BranchFieldOverrides, BranchWorktreeIntent } from "../../lib/branchConflict";
import type {
  SessionAttachmentInit,
  StartSessionRequestInit,
} from "../../hooks/useSessionAttachments";

export type SessionType = "tool" | "claude-cli" | "cursor-cli";

/**
 * Every form value `buildStartSessionRequest` reads. Named rather than passed one argument at a
 * time because the builder is the whole of the form's request shape and reads most of it.
 */
export interface CreateSessionFormValues {
  sessionToken: string;
  projectId: string;
  branchIntent: BranchWorktreeIntent;
  newBranchName: string;
  createRemoteBranch: boolean;
  selectedBaseBranch: string;
  selectedBranchToWorkOn: string;
  daemonInstanceId: string;
  sessionType: SessionType;
  toolPath: string;
  selectedAgentId: string;
  recipe: string;
  stackParent: string;
  stackNodeId: string;
  stackParentDaemonInstanceId: string;
  prStackBaseSessionId: string;
  model: string;
  permissionMode: string;
  dangerouslySkipPermissions: boolean;
  placementWithdrawsPermissionBypass: boolean;
  initialPrompt: string;
  sandbox: boolean;
  managedCodebase: boolean;
  sandboxedCodebase: boolean;
  isSplitCodebase: boolean;
  selectedAgentIds: string[];
  semanticIndex: boolean;
  codebaseDaemonInstanceId: string;
  sshConfigHost: string;
}

/**
 * Build one `StartSession` request for the current form state, with the branch fields optionally
 * overridden by a branch-conflict resolution (which re-runs the same creation under different
 * branch fields).
 */
export function buildStartSessionRequest(
  form: CreateSessionFormValues,
  branchOverrides: BranchFieldOverrides | null,
  requestAttachments: SessionAttachmentInit[],
): StartSessionRequestInit {
  const {
    sessionToken,
    projectId,
    branchIntent,
    newBranchName,
    createRemoteBranch,
    selectedBaseBranch,
    selectedBranchToWorkOn,
    daemonInstanceId,
    sessionType,
    toolPath,
    selectedAgentId,
    recipe,
    stackParent,
    stackNodeId,
    stackParentDaemonInstanceId,
    prStackBaseSessionId,
    model,
    permissionMode,
    dangerouslySkipPermissions,
    placementWithdrawsPermissionBypass,
    initialPrompt,
    sandbox,
    managedCodebase,
    sandboxedCodebase,
    isSplitCodebase,
    selectedAgentIds,
    semanticIndex,
    codebaseDaemonInstanceId,
    sshConfigHost,
  } = form;
  const commonParams = {
    sessionToken,
    projectId,
    branchWorktreeIntent: branchIntent,
    newBranchName,
    createRemoteBranch,
    selectedIntegrationBaseRef: selectedBaseBranch,
    selectedBranchToWorkOn,
    daemonInstanceId,
    // Ask to be refused rather than silently given `<branch>-1` when another session owns the
    // branch: this form has an operator to prompt. See docs/ft/daemon/session-branch-conflict.md.
    onBranchConflict: "reject",
    // Documents the daemon materializes before the agent starts. Empty for a form with nothing
    // attached, which is byte-for-byte the request this pane has always sent.
    attachments: requestAttachments,
    ...branchOverrides,
  };
  if (sessionType === "tool") {
    return {
      ...commonParams,
      toolPath,
      // The bare id the host knows it by — the option's value is qualified for the select's sake
      // only, and `daemonInstanceId` already carries the host beside it.
      agent: selectedAgentId,
      recipe,
      stackParent,
      stackNodeId,
      // No owning host: a tool session's chain base is resolved by the `tddy-coder` the daemon
      // spawns, against that process's own sessions tree, not by the daemon at start. Sending a
      // host here would name a routing the start never performs.
      stackParentDaemonInstanceId: "",
      // Only the tool branch can create an orchestrator, so only it can name a session to seed the
      // orchestrator's stack from. Sent only for the recipe whose picker offered it — the daemon
      // refuses a base session named beside any other recipe rather than dropping it silently, so a
      // choice made before switching recipes must not leak into the request.
      prStackBaseSessionId: recipe === "pr-stack" ? prStackBaseSessionId : "",
      sessionType: "",
      model,
      permissionMode: "",
      initialPrompt: "",
      sandbox: false,
    };
  }
  if (sessionType === "cursor-cli") {
    return {
      ...commonParams,
      toolPath: "",
      agent: "",
      recipe: managedCodebase ? recipe : "",
      stackParent,
      stackParentDaemonInstanceId,
      stackNodeId,
      sessionType: "cursor-cli",
      model,
      permissionMode: "",
      initialPrompt,
      sandbox,
      managedCodebase,
      specializedAgents: selectedAgentIds,
      semanticIndex,
      // cursor-agent has no tool allowlist, so a split codebase could only be suggested to it,
      // never enforced — the daemon refuses such a request. Both managed-codebase blocks share
      // state, so a host picked while the form was claude-cli must not survive the switch here.
      codebaseDaemonInstanceId: "",
    };
  }
  return {
    ...commonParams,
    toolPath: "",
    agent: "",
    // A recipe's tooling runs against a repository on the daemon hosting the agent, which a
    // split session does not have — the daemon refuses the combination. The form defaults
    // `recipe` to a non-empty value, so without this a split session would be created as a
    // request that cannot succeed.
    recipe: managedCodebase && !isSplitCodebase ? recipe : "",
    stackParent,
    stackParentDaemonInstanceId,
    stackNodeId,
    sessionType: "claude-cli",
    model,
    permissionMode,
    dangerouslySkipPermissions: placementWithdrawsPermissionBypass
      ? false
      : dangerouslySkipPermissions,
    initialPrompt,
    sandbox,
    managedCodebase,
    // The third placement. Choosing it cleared the two above and the codebase host in the form,
    // so they go out cleared here without being restated — the request says what the screen says.
    sandboxedCodebase,
    // Both ride along on any placement, split or jailed or neither, and on whether the codebase is
    // managed or not — an agent reads the codebase through its own placement, and the index is
    // built wherever the worktree is. Nothing gates them here: the picker and the toggle are shown
    // on every claude-cli placement, so what the form holds is what the screen showed.
    specializedAgents: selectedAgentIds,
    semanticIndex,
    // A remote worktree is reachable only through the mcp__tddy-tools__* proxy that managed
    // codebase installs, so a placement chosen before the toggle was switched off would name a
    // combination the daemon refuses.
    codebaseDaemonInstanceId: isSplitCodebase ? codebaseDaemonInstanceId : "",
    sshConfigHost,
  };
}

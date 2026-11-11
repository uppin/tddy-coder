import type { BranchFieldOverrides, BranchWorktreeIntent } from "./lib/branchConflict";
import { TDDY_CODER_LAUNCH, type SessionLaunch } from "./cliHarnessOptions";
import type {
  SessionAttachmentInit,
  StartSessionRequestInit,
} from "./hooks/useSessionAttachments";

/** A session type, as its daemon declared it (`ListAgentsResponse.session_types`). */
export type SessionType = string;

/**
 * What the agent behind a CLI session type may be asked for, derived in the pane from the
 * metadata the target daemon served for that agent (`ListAgents`): the params its launch
 * argv binds, the capabilities its spec declares, and whether a provider route can point
 * it elsewhere. The request builder reads only this — it names no agent.
 */
export interface CliAgentFeatures {
  bindsModel: boolean;
  bindsPermissionMode: boolean;
  bindsSkipPermissions: boolean;
  bindsPrompt: boolean;
  managedWorkflow: boolean;
  splitHost: boolean;
  providerRoutable: boolean;
}

/** An agent that may be asked for nothing: the fail-closed shape for an unserved type. */
export const NO_CLI_FEATURES: CliAgentFeatures = {
  bindsModel: false,
  bindsPermissionMode: false,
  bindsSkipPermissions: false,
  bindsPrompt: false,
  managedWorkflow: false,
  splitHost: false,
  providerRoutable: false,
};

/**
 * Whether the daemon cuts the session's worktree itself — and so can push the new branch to origin
 * at start. Every CLI harness session does; a session of the `tddy_coder` launch spawns tddy-coder,
 * which owns its own worktree.
 */
export function daemonCutsTheWorktree(sessionLaunch: SessionLaunch | undefined): boolean {
  return sessionLaunch !== TDDY_CODER_LAUNCH;
}

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
  /** How a session of `sessionType` is launched, as its daemon declared it. */
  sessionLaunch: SessionLaunch | undefined;
  /** What the agent behind `sessionType` may be asked for; absent for a tool session. */
  cliFeatures?: CliAgentFeatures;
  toolPath: string;
  selectedAgentId: string;
  recipe: string;
  stackParent: string;
  stackNodeId: string;
  stackParentDaemonInstanceId: string;
  prStackBaseSessionId: string;
  model: string;
  /** The provider account the harness runs on; empty or absent for its vendor. */
  providerAccountId?: string;
  /** The operator explicitly chose the vendor, overriding any default account. */
  providerBuiltIn?: boolean;
  permissionMode: string;
  dangerouslySkipPermissions: boolean;
  placementWithdrawsPermissionBypass: boolean;
  initialPrompt: string;
  sandbox: boolean;
  /** Run the agent inside tmux on the host. Sent as `run_in_tmux` for every CLI harness type. */
  runInTmux?: boolean;
  managedCodebase: boolean;
  sandboxedCodebase: boolean;
  isSplitCodebase: boolean;
  selectedAgentIds: string[];
  /** The picked agents, out of `selectedAgentIds`, whose conversations work on a clone of their own. */
  clonePlacedAgentIds: string[];
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
    providerAccountId,
    providerBuiltIn,
    initialPrompt,
    sandbox,
    managedCodebase,
    sandboxedCodebase,
    isSplitCodebase,
    selectedAgentIds,
    clonePlacedAgentIds,
    semanticIndex,
    codebaseDaemonInstanceId,
    sshConfigHost,
  } = form;
  const clonePlacedAgents = pickedClonePlacedAgents(clonePlacedAgentIds, selectedAgentIds);
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
  if (form.sessionLaunch === TDDY_CODER_LAUNCH) {
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
      // The session type the toggle chose: the daemon refuses an empty one rather than reading
      // it as a tool session.
      sessionType,
      model,
      // The account the agent runs on, or the vendor. The daemon refuses an agent that cannot run
      // on the account it is named with, rather than starting it on the vendor.
      providerAccountId,
      providerBuiltIn,
      permissionMode: "",
      initialPrompt: "",
      sandbox: false,
    };
  }
  const features = form.cliFeatures ?? NO_CLI_FEATURES;
  // A split placement only exists for an agent whose spec declares it can stand on one
  // host against a codebase on another; for any other agent a codebase host picked while
  // the form showed a different agent must not survive into the request.
  const split = features.splitHost && isSplitCodebase;
  return {
    ...commonParams,
    toolPath: "",
    agent: "",
    // A recipe's tooling runs against a repository on the daemon hosting the agent, which a
    // split session does not have — the daemon refuses the combination. The form defaults
    // `recipe` to a non-empty value, so without this a split session would be created as a
    // request that cannot succeed.
    recipe: features.managedWorkflow && managedCodebase && !split ? recipe : "",
    stackParent,
    stackParentDaemonInstanceId,
    stackNodeId,
    ...cliAgentChoices(form, features),
    sandbox,
    runInTmux: runInTmuxChoice(form),
    managedCodebase: features.managedWorkflow ? managedCodebase : false,
    // The third placement. Choosing it cleared the two above and the codebase host in the form,
    // so they go out cleared here without being restated — the request says what the screen says.
    sandboxedCodebase,
    codebaseDaemonInstanceId: split ? codebaseDaemonInstanceId : "",
    // The SSH host belongs to the managed block: it names the exec catalog's OpenSSH host
    // for a managed session, split or co-located.
    sshConfigHost: features.managedWorkflow && managedCodebase ? sshConfigHost : "",
  };
}

/**
 * Whether the request carries `run_in_tmux`: the operator's choice, for every CLI session type, and
 * never with the sandbox, the jailed codebase or a split placement, each of which the daemon
 * refuses together with it. (The pane clears a sandbox or jailed codebase left on from another
 * session type when it switches to an agent whose spec does not offer the placement —
 * `useCreateSessionPlacement` — so the form does not hold one there.)
 */
function runInTmuxChoice(form: CreateSessionFormValues): boolean {
  return (
    (form.runInTmux ?? false) && !form.sandbox && !form.sandboxedCodebase && !form.isSplitCodebase
  );
}

/**
 * What the operator chose for the form's CLI agent, independent of where it runs — each field
 * sent only when the agent's served metadata says it may be asked for it: the model and the
 * first prompt only when its launch argv binds them, the permission fields only when it binds
 * a permission mode or the bypass, the provider only when a route can point it elsewhere.
 * Shared by the New-session request and the main start spec so the two cannot drift.
 */
function cliAgentChoices(form: CreateSessionFormValues, features: CliAgentFeatures) {
  return {
    sessionType: form.sessionType,
    model: features.bindsModel ? form.model : "",
    providerAccountId: features.providerRoutable ? form.providerAccountId : undefined,
    providerBuiltIn: features.providerRoutable ? form.providerBuiltIn : undefined,
    permissionMode: features.bindsPermissionMode ? form.permissionMode : "",
    dangerouslySkipPermissions: features.bindsSkipPermissions
      ? form.placementWithdrawsPermissionBypass
        ? false
        : form.dangerouslySkipPermissions
      : undefined,
    initialPrompt: features.bindsPrompt ? form.initialPrompt : "",
    // Both ride along on any placement, split or jailed or neither, and on whether the codebase is
    // managed or not — an agent reads the codebase through its own placement, and the index is
    // built wherever the worktree is. Nothing gates them here: the picker and the toggle are shown
    // on every CLI placement, so what the form holds is what the screen showed.
    specializedAgents: form.selectedAgentIds,
    clonePlacedAgents: pickedClonePlacedAgents(form.clonePlacedAgentIds, form.selectedAgentIds),
    semanticIndex: form.semanticIndex,
  } satisfies StartSessionRequestInit;
}

/**
 * The start spec the main variant of the form sends in `EnsureProjectHomeSession.start`: the
 * CLI request without the fields the daemon owns for main — project, branch, placement and
 * stack — so the spec says only what the operator chose.
 */
export function buildProjectHomeStartSpec(form: CreateSessionFormValues): StartSessionRequestInit {
  return cliAgentChoices(form, form.cliFeatures ?? NO_CLI_FEATURES);
}

/** A placement names an agent that is picked: one left on a since-unpicked agent goes nowhere. */
function pickedClonePlacedAgents(clonePlacedAgentIds: string[], selectedAgentIds: string[]): string[] {
  return clonePlacedAgentIds.filter((id) => selectedAgentIds.includes(id));
}

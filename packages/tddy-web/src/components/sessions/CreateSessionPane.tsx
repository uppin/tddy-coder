import React, { useEffect, useState } from "react";
import { flushSync } from "react-dom";
import type { Client } from "@connectrpc/connect";
import type { AgentInfo, BranchConflict, ConnectionService, ProjectEntry, SessionEntry, SubagentInfo, ToolInfo } from "../../gen/connection_pb";
import { localBranchName } from "../../lib/branchNames";
import type { BaseBranchOption } from "./prstack/baseBranchChoice";
import {
  startSessionOverridesFor,
  type BranchConflictResolution,
  type BranchFieldOverrides,
  type BranchWorktreeIntent,
} from "../../lib/branchConflict";
import { prStackOrchestrators } from "../../utils/stackParents";
import { useDaemons, useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useAgentModels } from "../../rpc/useAgentModels";
import {
  useSessionAttachments,
  type SessionAttachmentInit,
  type StartSessionRequestInit,
} from "../../hooks/useSessionAttachments";
import { Button } from "../ui/button";
import { BranchConflictDialog } from "./BranchConflictDialog";
import { AttachmentDropZone } from "./attachments/AttachmentDropZone";
import { HostDocumentPicker } from "./attachments/HostDocumentPicker";
import { SessionAttachmentList } from "./attachments/SessionAttachmentList";

/** Pseudo-agent key used to fetch the claude-cli session type's model catalog. */
const CLAUDE_CLI_AGENT = "claude-cli";
const CURSOR_CLI_AGENT = "cursor-cli";

const WORKFLOW_RECIPES = [
  "tdd",
  "tdd-small",
  "bugfix",
  "free-prompting",
  "grill-me",
  "review",
  "merge-pr",
  "pr-stack",
] as const;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ConnectionClient = Client<typeof ConnectionService>;

type SessionType = "tool" | "claude-cli" | "cursor-cli";
type BranchIntent = BranchWorktreeIntent;

/**
 * Optional pre-fill for the form's fields. Used when the pane is opened from a context that already
 * knows what the session should look like (e.g. the PR-stack "Start session" flow pre-fills the
 * branch, prompt, and stack parent). Any field left unset keeps the form's own default.
 */
export type CreateSessionInitialValues = Partial<{
  sessionType: SessionType;
  projectId: string;
  recipe: string;
  model: string;
  permissionMode: string;
  dangerouslySkipPermissions: boolean;
  stackParent: string;
  branchIntent: BranchIntent;
  newBranchName: string;
  /**
   * Existing branch to pre-select in "Work on existing branch" mode — e.g. the branch a planned PR
   * already owns, which is resumed rather than re-created. Survives the async `ListProjectBranches`
   * load, which would otherwise auto-select the project's first branch.
   *
   * Named the way the rest of the domain names a branch (`feature/x`); the picker's own options are
   * remote-tracking refs (`origin/feature/x`) and are matched on the local name behind them.
   */
  selectedBranch: string;
  /** Pre-check state for the "Create Remote Branch" toggle (new-branch mode). Defaults to checked. */
  createRemoteBranch: boolean;
  /** Concrete base branch shown in the new-branch option: "New branch from base: <baseBranchLabel>". */
  baseBranchLabel: string;
  /**
   * Ordered base-branch options for the "Base branch" selector (planned-PR child sessions). Each
   * option carries the ref it submits and its caption separately: a legacy project's project default
   * is the empty ref the daemon resolves itself, which needs a label naming it rather than a blank
   * option (see `baseBranchChoice`).
   */
  baseBranchOptions: BaseBranchOption[];
  /**
   * Pre-selected base branch in the "Base branch" selector — the caller's derived base, which is
   * always one of `baseBranchOptions`.
   */
  selectedBaseBranch: string;
  initialPrompt: string;
  daemonInstanceId: string;
  /**
   * Absolute path to a local git checkout to reuse as the session worktree (sets
   * `StartSession.repo_path`). Used by the peer-agent spawn flow so a peer runs on the SAME worktree
   * as the orchestrating session — no new git worktree is created and no branch is checked out, so
   * branch selection is irrelevant in that flow (see `CreateSessionPaneProps.peerMode`).
   */
  repoPath: string;
}>;

export interface CreateSessionPaneProps {
  client: ConnectionClient;
  sessionToken: string;
  onCancel: () => void;
  onCreated: (sessionId: string) => void;
  initialValues?: CreateSessionInitialValues;
  /**
   * Peer-agent spawn mode: the new session runs on the SAME worktree as an orchestrating session
   * (via `initialValues.repoPath`), so branch selection (`branchIntent` / `newBranchName` /
   * `selectedBranch` / `createRemoteBranch`) is hidden — those controls have no effect when
   * `repo_path` is set. The submit still sends `stackParent` (from `initialValues`) and `repoPath`.
   */
  peerMode?: boolean;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const inputClass =
  "w-full rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

const labelClass = "block text-sm mb-1 text-muted-foreground";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function CreateSessionPane({
  client,
  sessionToken,
  onCancel,
  onCreated,
  initialValues,
  peerMode = false,
}: CreateSessionPaneProps) {
  const daemons = useDaemons();
  const { selectedInstanceId } = useSelectedDaemon();

  const [sessionType, setSessionType] = useState<SessionType>(initialValues?.sessionType ?? "tool");
  const [projectId, setProjectId] = useState(initialValues?.projectId ?? "");
  const [agent, setAgent] = useState("");
  const [recipe, setRecipe] = useState(initialValues?.recipe ?? "tdd");
  const [stackParent, setStackParent] = useState(initialValues?.stackParent ?? "");
  const [toolPath, setToolPath] = useState("");
  const [model, setModel] = useState(initialValues?.model ?? "");
  const [permissionMode, setPermissionMode] = useState(initialValues?.permissionMode ?? "auto");
  const [dangerouslySkipPermissions, setDangerouslySkipPermissions] = useState(
    initialValues?.dangerouslySkipPermissions ?? false,
  );
  const [sandbox, setSandbox] = useState(false);
  const [initialPrompt, setInitialPrompt] = useState(initialValues?.initialPrompt ?? "");
  const [branchIntent, setBranchIntent] = useState<BranchIntent>(
    initialValues?.branchIntent ?? "new_branch_from_base",
  );
  const [newBranchName, setNewBranchName] = useState(initialValues?.newBranchName ?? "");
  const [createRemoteBranch, setCreateRemoteBranch] = useState(
    initialValues?.createRemoteBranch ?? true,
  );
  const [baseBranchOptions] = useState<BaseBranchOption[]>(initialValues?.baseBranchOptions ?? []);
  const [selectedBaseBranch, setSelectedBaseBranch] = useState<string>(
    initialValues?.selectedBaseBranch ?? "",
  );
  // Read out of `initialValues` once: the branch load effect below needs it as a dependency, and
  // `initialValues` itself is a fresh object on every render of the caller.
  const preFilledBranchToWorkOn = initialValues?.selectedBranch ?? "";
  const [selectedBranchToWorkOn, setSelectedBranchToWorkOn] = useState(preFilledBranchToWorkOn);
  // Which daemon/host runs the session. Defaults to the pre-filled host, else the selected daemon,
  // else empty (which the daemon treats as "run locally on the connected daemon"). An empty
  // pre-filled host falls through to the selected daemon so the Host <select>'s displayed option
  // matches the value it will submit.
  const [daemonInstanceId, setDaemonInstanceId] = useState(
    initialValues?.daemonInstanceId || selectedInstanceId || "",
  );

  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [subagents, setSubagents] = useState<SubagentInfo[]>([]);
  const [selectedSubagents, setSelectedSubagents] = useState<string[]>([]);
  const [managedCodebase, setManagedCodebase] = useState(false);
  const [semanticIndex, setSemanticIndex] = useState(false);
  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [remoteBranches, setRemoteBranches] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Set when the daemon refused a creation because another session already owns the requested branch.
  // The form stays mounted behind the prompt, so cancelling returns to it with its values intact.
  const [branchConflict, setBranchConflict] = useState<BranchConflict | null>(null);

  // The model catalog is enumerated per selected backend: the chosen agent for tool sessions, and
  // the "claude-cli" pseudo-agent for the Claude CLI session type.
  const modelAgentKey =
    sessionType === "claude-cli"
      ? CLAUDE_CLI_AGENT
      : sessionType === "cursor-cli"
        ? CURSOR_CLI_AGENT
        : agent;
  const agentModels = useAgentModels(client, sessionToken, modelAgentKey, daemonInstanceId);

  // Reset the model selection to the backend's advertised default whenever the catalog changes
  // (agent switch, session-type switch). Empty while loading or on a failed probe.
  useEffect(() => {
    setModel(agentModels.defaultModel);
  }, [agentModels.defaultModel]);

  const toggleSubagent = (name: string) => {
    setSelectedSubagents((prev) =>
      prev.includes(name) ? prev.filter((n) => n !== name) : [...prev, name],
    );
  };

  // Load data on mount
  useEffect(() => {
    let cancelled = false;

    // Fetch sessions separately so a network failure doesn't block the rest of the form.
    client
      .listSessions({ sessionToken })
      .then((resp) => {
        if (cancelled) return;
        const loadedSessions = prStackOrchestrators(resp.sessions as SessionEntry[]);
        setSessions(loadedSessions);
      })
      .catch(() => {
        // Session list is best-effort; failing to fetch it just hides the parent picker.
      });

    // Fetch subagents separately (best-effort, like sessions above) — a daemon that doesn't
    // implement ListSubagents yet, or a test double that doesn't stub it, must not block the
    // core project/agent/tool fields from loading.
    client
      .listSubagents({})
      .then((resp) => {
        if (!cancelled) {
          setSubagents(resp.subagents as SubagentInfo[]);
        }
      })
      .catch(() => {
        // Specialized subagents are best-effort; failing to fetch them just leaves the
        // "Managed codebase" section with no options to pick.
      });

    Promise.all([
      client.listProjects({ sessionToken }),
      client.listAgents({}),
      client.listTools({}),
    ])
      .then(([projectsResp, agentsResp, toolsResp]) => {
        if (cancelled) return;

        const loadedProjects = projectsResp.projects as ProjectEntry[];
        const loadedAgents = agentsResp.agents as AgentInfo[];
        const loadedTools = toolsResp.tools as ToolInfo[];

        setProjects(loadedProjects);
        setAgents(loadedAgents);
        setTools(loadedTools);

        // Auto-select agent and toolPath.
        if (loadedAgents.length > 0) {
          setAgent(loadedAgents[0]!.id);
        }
        if (loadedTools.length > 0) {
          setToolPath(loadedTools[0]!.path);
        }
        // Auto-select projectId when there is exactly one choice — no meaningful decision.
        if (loadedProjects.length === 1) {
          setProjectId(loadedProjects[0]!.projectId);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          console.debug("[CreateSessionPane] load error", err);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken]);

  // Load branches when projectId changes and intent is work_on_selected_branch
  useEffect(() => {
    if (!projectId || branchIntent !== "work_on_selected_branch") return;
    let cancelled = false;
    client
      .listProjectBranches({ sessionToken, projectId, daemonInstanceId })
      .then((resp) => {
        if (!cancelled) {
          setRemoteBranches(resp.branches);
          if (resp.branches.length > 0) {
            // A pre-filled branch wins over the default first entry, but only while the project
            // actually offers it — otherwise the <select> would hold a value none of its options
            // match, and submit would send a branch this project does not have.
            //
            // Matched on the *local* branch name behind each option, because `ListProjectBranches`
            // lists remote-tracking refs (`<remote>/<branch>`) while callers name the branch the way
            // the rest of the domain does. Comparing the raw strings never matches, and the
            // pre-fill then degrades silently into an unrelated branch — the operator resumes the
            // wrong branch with no warning. The remote is the daemon-resolved default
            // (`resp.defaultRemote`), so a non-`origin` project strips the right prefix.
            const remote = resp.defaultRemote || "origin";
            const wanted = localBranchName(preFilledBranchToWorkOn, remote);
            const offered = resp.branches.find((b) => localBranchName(b, remote) === wanted);
            setSelectedBranchToWorkOn(offered ?? resp.branches[0]!);
          }
        }
      })
      .catch((err) => {
        if (!cancelled) {
          console.debug("[CreateSessionPane] listProjectBranches error", err);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, projectId, branchIntent, daemonInstanceId, preFilledBranchToWorkOn]);

  // In peer mode the project and host are locked to the orchestrating session (the pane reuses its
  // worktree via `repo_path`), so the Project/Host selectors are hidden and submit must send the
  // frozen `initialValues` values — not the live form state, which the mount-time auto-select could
  // have overridden with a different single project.
  const effectiveProjectId = peerMode ? (initialValues?.projectId ?? "") : projectId;
  const effectiveDaemonInstanceId = peerMode
    ? (initialValues?.daemonInstanceId ?? "")
    : daemonInstanceId;

  // The attach rows and everything that follows from them: the effective size cap, the refusal shown
  // next to a bad row, the upload of local files on submit, and the streamed start that reports the
  // host's materialization progress.
  const {
    attachments,
    progress: attachmentProgress,
    stagingDaemonInstanceId,
    problem: attachmentProblem,
    pickRefusal,
    hostDocPickerOpen,
    attachFiles,
    attachHostDocument,
    renameAttachment,
    removeAttachment,
    openHostDocPicker,
    closeHostDocPicker,
    resetProgress: resetAttachmentProgress,
    stageAttachments,
    startSessionStreamed,
  } = useSessionAttachments({
    client,
    sessionToken,
    sessionDaemonInstanceId: effectiveDaemonInstanceId,
  });

  const isSubmitEnabled = (() => {
    if (submitting) return false;
    // An attachment the daemon would refuse (duplicate or unsafe basename) fails the whole creation,
    // so it is refused in the form instead.
    if (attachmentProblem !== null) return false;
    // A model is always required and comes from the daemon-advertised catalog; a failed/loading
    // probe leaves `model` empty, which disables Create (no fallback).
    if (sessionType === "tool") {
      return Boolean(effectiveProjectId && agent && toolPath && model);
    }
    return Boolean(effectiveProjectId && model);
  })();

  /**
   * Build one `StartSession` request for the current form state, with the branch fields optionally
   * overridden by a branch-conflict resolution (which re-runs the same creation under different
   * branch fields).
   */
  const startSessionRequest = (
    branchOverrides: BranchFieldOverrides | null,
    requestAttachments: SessionAttachmentInit[],
  ): StartSessionRequestInit => {
    // In peer mode the new session runs on the SAME worktree as the orchestrating session
    // (via repo_path), so no git worktree is created and no branch is checked out — branch fields
    // are irrelevant and kept empty.
    const peerRepoPath = peerMode ? (initialValues?.repoPath ?? "") : "";
    const commonParams = {
      sessionToken,
      projectId: effectiveProjectId,
      branchWorktreeIntent: peerMode ? "" : branchIntent,
      newBranchName: peerMode ? "" : newBranchName,
      createRemoteBranch: peerMode ? false : createRemoteBranch,
      selectedIntegrationBaseRef: peerMode ? "" : selectedBaseBranch,
      selectedBranchToWorkOn: peerMode ? "" : selectedBranchToWorkOn,
      daemonInstanceId: effectiveDaemonInstanceId,
      repoPath: peerRepoPath,
      // Ask to be refused rather than silently given `<branch>-1` when another session owns the
      // branch: this form has an operator to prompt. A peer creates no branch at all, so there is
      // nothing to conflict over. See docs/ft/daemon/session-branch-conflict.md.
      onBranchConflict: peerMode ? "" : "reject",
      // Documents the daemon materializes before the agent starts. Empty for a form with nothing
      // attached, which is byte-for-byte the request this pane has always sent.
      attachments: requestAttachments,
      ...branchOverrides,
    };
    if (sessionType === "tool") {
      return {
        ...commonParams,
        toolPath,
        agent,
        recipe,
        stackParent,
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
        sessionType: "cursor-cli",
        model,
        permissionMode: "",
        initialPrompt,
        sandbox,
        managedCodebase,
        specializedAgents: managedCodebase ? selectedSubagents : [],
        semanticIndex: managedCodebase ? semanticIndex : false,
      };
    }
    return {
      ...commonParams,
      toolPath: "",
      agent: "",
      recipe: managedCodebase ? recipe : "",
      stackParent,
      sessionType: "claude-cli",
      model,
      permissionMode,
      dangerouslySkipPermissions,
      initialPrompt,
      sandbox,
      managedCodebase,
      // Only send subagents when managed codebase is enabled — the picker is hidden otherwise,
      // so a selection made before unchecking the toggle must not leak into the request.
      specializedAgents: managedCodebase ? selectedSubagents : [],
      semanticIndex: managedCodebase ? semanticIndex : false,
    };
  };

  const submitCreation = async (branchOverrides: BranchFieldOverrides | null) => {
    // Use flushSync to commit the submitting state synchronously before the async fetch starts.
    // This ensures the Create button is visibly disabled in the very next render cycle, even
    // if the network response arrives quickly (e.g. in tests with a fast stub).
    flushSync(() => {
      setSubmitting(true);
      setError(null);
      resetAttachmentProgress();
    });
    try {
      // Uploads only what is not already on the staging host, so answering a branch-conflict prompt
      // re-runs the creation without re-sending bytes that already arrived.
      const requestAttachments: SessionAttachmentInit[] = await stageAttachments();
      const request = startSessionRequest(branchOverrides, requestAttachments);
      // Streaming only buys per-attachment progress, so a creation with nothing attached keeps using
      // the unary RPC every other client uses.
      const res =
        requestAttachments.length === 0
          ? await client.startSession(request)
          : await startSessionStreamed(request);
      if (res === null) {
        // The form unmounted while the host was still working; it owns no navigation any more.
        return;
      }
      if (res.branchConflict) {
        // Another session owns the branch and nothing was created — ask the operator how to proceed
        // instead of navigating to a session that does not exist.
        setBranchConflict(res.branchConflict);
        return;
      }
      onCreated(res.sessionId);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setSubmitting(false);
    }
  };

  const handleSubmit = () => submitCreation(null);

  /**
   * Apply the operator's answer to the branch-conflict prompt. "Switch" submits nothing: it hands the
   * owning session's id to `onCreated`, which is what selects and attaches a session. The other two
   * choices re-run creation with the branch fields that choice implies.
   */
  const resolveBranchConflict = (
    conflict: BranchConflict,
    resolution: BranchConflictResolution,
  ) => {
    const branchOverrides = startSessionOverridesFor(resolution, conflict);
    setBranchConflict(null);
    if (branchOverrides === null) {
      // `owner` is only optional because proto3 message fields always are in the generated types —
      // a reported conflict always names the session that holds the branch.
      onCreated(conflict.owner?.sessionId ?? "");
      return;
    }
    void submitCreation(branchOverrides);
  };

  // Model selector — shared by both session types, populated from the daemon-advertised catalog for
  // the current backend. While the probe is in flight it shows a loading line; a failed probe shows
  // an inline error and renders no select (so `model` stays empty and Create is disabled).
  const modelField = (
    <div>
      <label className={labelClass} htmlFor="create-session-model">
        Model
      </label>
      {agentModels.loading ? (
        <p data-testid="create-session-model-loading" className="text-sm text-muted-foreground">
          Loading models…
        </p>
      ) : agentModels.error !== null ? (
        <p data-testid="create-session-model-error" className="text-sm text-destructive">
          {agentModels.error}
        </p>
      ) : (
        <select
          id="create-session-model"
          data-testid="create-session-model-select"
          className={inputClass}
          value={model}
          onChange={(e) => setModel(e.target.value)}
        >
          {agentModels.models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>
      )}
    </div>
  );

  return (
    <div
      data-testid="create-session-pane"
      className="flex flex-col h-full overflow-y-auto p-4 space-y-4"
    >
      <h2 className="text-sm font-semibold">New session</h2>

      {/* Session type toggle */}
      <div className="flex gap-2">
        <button
          type="button"
          data-testid="create-session-type-tool"
          aria-pressed={sessionType === "tool"}
          onClick={() => setSessionType("tool")}
          className={`px-3 py-1.5 rounded-md text-sm border transition-colors ${
            sessionType === "tool"
              ? "bg-primary text-primary-foreground border-primary"
              : "bg-background text-foreground border-input hover:bg-muted"
          }`}
        >
          Tool
        </button>
        <button
          type="button"
          data-testid="create-session-type-claude-cli"
          aria-pressed={sessionType === "claude-cli"}
          onClick={() => setSessionType("claude-cli")}
          className={`px-3 py-1.5 rounded-md text-sm border transition-colors ${
            sessionType === "claude-cli"
              ? "bg-primary text-primary-foreground border-primary"
              : "bg-background text-foreground border-input hover:bg-muted"
          }`}
        >
          Claude CLI
        </button>
        <button
          type="button"
          data-testid="create-session-type-cursor-cli"
          aria-pressed={sessionType === "cursor-cli"}
          onClick={() => setSessionType("cursor-cli")}
          className={`px-3 py-1.5 rounded-md text-sm border transition-colors ${
            sessionType === "cursor-cli"
              ? "bg-primary text-primary-foreground border-primary"
              : "bg-background text-foreground border-input hover:bg-muted"
          }`}
        >
          Cursor CLI
        </button>
      </div>

      {/* Host — which daemon runs the session. Only shown when the common room advertises daemons.
          Hidden in peer mode: the peer runs on the orchestrator's host (locked via initialValues). */}
      {daemons.length > 0 && !peerMode && (
        <div>
          <label className={labelClass} htmlFor="create-session-host">
            Host
          </label>
          <select
            id="create-session-host"
            data-testid="create-session-host-select"
            className={inputClass}
            value={daemonInstanceId}
            onChange={(e) => setDaemonInstanceId(e.target.value)}
          >
            {daemons.map((d) => (
              <option key={d.instanceId} value={d.instanceId}>
                {d.label}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Project — hidden in peer mode: the peer runs on the orchestrator's worktree, so its project
          is locked to the orchestrator's (frozen via initialValues, sent as `effectiveProjectId`). */}
      {!peerMode && (
        <div>
          <label className={labelClass} htmlFor="create-session-project">
            Project
          </label>
          <select
            id="create-session-project"
            data-testid="create-session-project-select"
            className={inputClass}
            value={projectId}
            onChange={(e) => setProjectId(e.target.value)}
          >
            <option value="" disabled>
              {projects.length === 0 ? "No projects available" : "Select a project…"}
            </option>
            {projects.map((p) => (
              <option key={p.projectId} value={p.projectId}>
                {p.name || p.projectId}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Tool session fields */}
      {sessionType === "tool" && (
        <>
          <div>
            <label className={labelClass} htmlFor="create-session-agent">
              Agent
            </label>
            <select
              id="create-session-agent"
              data-testid="create-session-agent-select"
              className={inputClass}
              value={agent}
              onChange={(e) => setAgent(e.target.value)}
            >
              {agents.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.label || a.id}
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className={labelClass} htmlFor="create-session-recipe">
              Recipe
            </label>
            <select
              id="create-session-recipe"
              data-testid="create-session-recipe-select"
              className={inputClass}
              value={recipe}
              onChange={(e) => setRecipe(e.target.value)}
            >
              {WORKFLOW_RECIPES.map((r) => (
                <option key={r} value={r}>
                  {r}
                </option>
              ))}
            </select>
          </div>

          {modelField}
        </>
      )}

      {/* Cursor CLI session fields */}
      {sessionType === "cursor-cli" && (
        <>
          {modelField}
          <div>
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                data-testid="create-session-sandbox-toggle"
                type="checkbox"
                className="h-4 w-4 rounded border-input"
                checked={sandbox}
                onChange={(e) => setSandbox(e.target.checked)}
              />
              Sandbox
            </label>
          </div>
          <div>
            <label className={labelClass} htmlFor="create-session-initial-prompt">
              Initial prompt
            </label>
            <textarea
              id="create-session-initial-prompt"
              data-testid="create-session-initial-prompt-input"
              className={`${inputClass} resize-y`}
              rows={3}
              value={initialPrompt}
              onChange={(e) => setInitialPrompt(e.target.value)}
              placeholder="Optional initial prompt"
            />
          </div>
          <div>
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                data-testid="create-session-managed-codebase-toggle"
                type="checkbox"
                className="h-4 w-4 rounded border-input"
                checked={managedCodebase}
                onChange={(e) => {
                  setManagedCodebase(e.target.checked);
                  if (!e.target.checked) setSemanticIndex(false);
                }}
              />
              Managed codebase
            </label>
            {managedCodebase && (
              <div className="mt-2 space-y-3 pl-4">
                <div>
                  <label className={labelClass} htmlFor="create-session-recipe">
                    Recipe
                  </label>
                  <select
                    id="create-session-recipe"
                    data-testid="create-session-recipe-select"
                    className={inputClass}
                    value={recipe}
                    onChange={(e) => setRecipe(e.target.value)}
                  >
                    {WORKFLOW_RECIPES.map((r) => (
                      <option key={r} value={r}>
                        {r}
                      </option>
                    ))}
                  </select>
                </div>
                <div
                  data-testid="create-session-managed-codebase-section"
                  className="space-y-1"
                >
                  {subagents.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      No specialized subagents available
                    </p>
                  ) : (
                    subagents.map((sa) => (
                      <label
                        key={sa.name}
                        className="flex items-center gap-2 text-sm text-muted-foreground"
                      >
                        <input
                          data-testid={`create-session-subagent-checkbox-${sa.name}`}
                          type="checkbox"
                          className="h-4 w-4 rounded border-input"
                          checked={selectedSubagents.includes(sa.name)}
                          onChange={() => toggleSubagent(sa.name)}
                        />
                        {sa.label || sa.name}
                      </label>
                    ))
                  )}
                </div>
                <div>
                  <label className="flex items-center gap-2 text-sm text-muted-foreground">
                    <input
                      data-testid="create-session-semantic-index-toggle"
                      type="checkbox"
                      className="h-4 w-4 rounded border-input"
                      checked={semanticIndex}
                      onChange={(e) => setSemanticIndex(e.target.checked)}
                    />
                    Semantic index
                  </label>
                </div>
              </div>
            )}
          </div>
        </>
      )}

      {/* Claude CLI session fields */}
      {sessionType === "claude-cli" && (
        <>
          {modelField}

          <div>
            <label className={labelClass} htmlFor="create-session-permission-mode">
              Permission mode
            </label>
            <select
              id="create-session-permission-mode"
              data-testid="create-session-permission-mode-select"
              className={inputClass}
              value={permissionMode}
              onChange={(e) => setPermissionMode(e.target.value)}
              disabled={dangerouslySkipPermissions}
            >
              <option value="auto">auto</option>
              <option value="default">default</option>
              <option value="acceptEdits">acceptEdits</option>
              <option value="plan">plan</option>
              <option value="bypassPermissions">bypassPermissions</option>
            </select>
          </div>

          <div>
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                data-testid="create-session-dangerously-skip-permissions-toggle"
                type="checkbox"
                className="h-4 w-4 rounded border-input"
                checked={dangerouslySkipPermissions}
                onChange={(e) => setDangerouslySkipPermissions(e.target.checked)}
              />
              Dangerously skip permissions
            </label>
          </div>

          <div>
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                data-testid="create-session-sandbox-toggle"
                type="checkbox"
                className="h-4 w-4 rounded border-input"
                checked={sandbox}
                onChange={(e) => setSandbox(e.target.checked)}
              />
              Sandbox
            </label>
          </div>

          <div>
            <label className={labelClass} htmlFor="create-session-initial-prompt">
              Initial prompt
            </label>
            <textarea
              id="create-session-initial-prompt"
              data-testid="create-session-initial-prompt-input"
              className={`${inputClass} resize-y`}
              rows={3}
              value={initialPrompt}
              onChange={(e) => setInitialPrompt(e.target.value)}
              placeholder="Optional initial prompt"
            />
          </div>

          {/* Managed codebase — an explicit toggle that, when on, makes the session workflow-aware
              (recipe picker) and lets the user attach specialized subagents.
              See docs/ft/coder/managed-codebase-workflow.md. */}
          <div>
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                data-testid="create-session-managed-codebase-toggle"
                type="checkbox"
                className="h-4 w-4 rounded border-input"
                checked={managedCodebase}
                onChange={(e) => {
                  setManagedCodebase(e.target.checked);
                  if (!e.target.checked) setSemanticIndex(false);
                }}
              />
              Managed codebase
            </label>
            {managedCodebase && (
              <div className="mt-2 space-y-3 pl-4">
                <div>
                  <label className={labelClass} htmlFor="create-session-recipe">
                    Recipe
                  </label>
                  <select
                    id="create-session-recipe"
                    data-testid="create-session-recipe-select"
                    className={inputClass}
                    value={recipe}
                    onChange={(e) => setRecipe(e.target.value)}
                  >
                    {WORKFLOW_RECIPES.map((r) => (
                      <option key={r} value={r}>
                        {r}
                      </option>
                    ))}
                  </select>
                </div>
                <div
                  data-testid="create-session-managed-codebase-section"
                  className="space-y-1"
                >
                  {subagents.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      No specialized subagents available
                    </p>
                  ) : (
                    subagents.map((sa) => (
                      <label
                        key={sa.name}
                        className="flex items-center gap-2 text-sm text-muted-foreground"
                      >
                        <input
                          data-testid={`create-session-subagent-checkbox-${sa.name}`}
                          type="checkbox"
                          className="h-4 w-4 rounded border-input"
                          checked={selectedSubagents.includes(sa.name)}
                          onChange={() => toggleSubagent(sa.name)}
                        />
                        {sa.label || sa.name}
                      </label>
                    ))
                  )}
                </div>
                <div>
                  <label className="flex items-center gap-2 text-sm text-muted-foreground">
                    <input
                      data-testid="create-session-semantic-index-toggle"
                      type="checkbox"
                      className="h-4 w-4 rounded border-input"
                      checked={semanticIndex}
                      onChange={(e) => setSemanticIndex(e.target.checked)}
                    />
                    Semantic index
                  </label>
                </div>
              </div>
            )}
          </div>
        </>
      )}

      {/* PR stack parent picker — shown for both session types when orchestrators are available.
          Hidden in peer mode: the peer's parent is locked to the orchestrating session. */}
      {sessions.length > 0 && !peerMode && (
        <div>
          <label className={labelClass} htmlFor="create-session-stack-parent">
            PR stack parent
          </label>
          <select
            id="create-session-stack-parent"
            data-testid="create-session-stack-parent-select"
            className={inputClass}
            value={stackParent}
            onChange={(e) => setStackParent(e.target.value)}
          >
            <option value="">None (standalone session)</option>
            {sessions.map((s) => (
              <option key={s.sessionId} value={s.sessionId}>
                {s.sessionId}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Branch intent — hidden in peer mode: the peer runs on the orchestrator's worktree via
          repo_path, so no git worktree is created and no branch is checked out. */}
      {!peerMode && (
        <>
          <div>
            <label className={labelClass} htmlFor="create-session-branch-intent">
              Branch mode
            </label>
            <select
              id="create-session-branch-intent"
              data-testid="create-session-branch-intent-select"
              className={inputClass}
              value={branchIntent}
              onChange={(e) => setBranchIntent(e.target.value as BranchIntent)}
            >
              <option value="new_branch_from_base">
                {`New branch from base${
                  initialValues?.baseBranchLabel ? `: ${initialValues.baseBranchLabel}` : ""
                }`}
              </option>
              <option value="work_on_selected_branch">Work on existing branch</option>
            </select>
          </div>

          {!peerMode && initialValues?.stackParent && baseBranchOptions.length > 0 && (
            <div>
              <label className={labelClass} htmlFor="create-session-base-branch">
                Base branch
              </label>
              <select
                id="create-session-base-branch"
                data-testid="create-session-base-branch-select"
                className={inputClass}
                value={selectedBaseBranch}
                onChange={(e) => setSelectedBaseBranch(e.target.value)}
              >
                {baseBranchOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </div>
          )}

          {branchIntent === "new_branch_from_base" && (
            <div>
              <label className={labelClass} htmlFor="create-session-new-branch-name">
                New branch name
              </label>
              <input
                id="create-session-new-branch-name"
                data-testid="create-session-new-branch-name-input"
                type="text"
                className={inputClass}
                value={newBranchName}
                onChange={(e) => setNewBranchName(e.target.value)}
                placeholder="e.g. feature/my-work"
              />
              {/* Only the claude-cli / cursor-cli spawn paths create the worktree in-daemon and can push
                  it; a "tool" session spawns tddy-coder, which owns its own worktree — so we don't offer
                  the toggle there rather than show a checked box that silently does nothing. */}
              {(sessionType === "claude-cli" || sessionType === "cursor-cli") && (
                <label className="mt-2 flex items-center gap-2 text-sm text-muted-foreground">
                  <input
                    data-testid="create-session-create-remote-branch-toggle"
                    type="checkbox"
                    className="h-4 w-4"
                    checked={createRemoteBranch}
                    onChange={(e) => setCreateRemoteBranch(e.target.checked)}
                  />
                  Create Remote Branch
                </label>
              )}
            </div>
          )}

          {branchIntent === "work_on_selected_branch" && (
            <div>
              <label className={labelClass} htmlFor="create-session-branch-to-work-on">
                Branch to work on
              </label>
              <select
                id="create-session-branch-to-work-on"
                data-testid="create-session-branch-to-work-on-select"
                className={inputClass}
                value={selectedBranchToWorkOn}
                onChange={(e) => setSelectedBranchToWorkOn(e.target.value)}
              >
                {remoteBranches.map((b) => (
                  <option key={b} value={b}>
                    {b}
                  </option>
                ))}
              </select>
            </div>
          )}
        </>
      )}

      {/* Attachments — documents the daemon materializes into artifacts/attachments/ before the
          agent starts. Shown for every session type and in peer mode, because the daemon
          materializes them for all of them. See docs/ft/coder/session-attachments.md. */}
      <AttachmentDropZone
        onFilesPicked={attachFiles}
        onPickHostDocument={openHostDocPicker}
        disabled={submitting}
      >
        <SessionAttachmentList
          attachments={attachments}
          progress={attachmentProgress}
          onRename={renameAttachment}
          onRemove={removeAttachment}
          disabled={submitting}
        />
        {(attachmentProblem ?? pickRefusal) !== null && (
          <p data-testid="create-session-attachment-error" className="text-sm text-destructive">
            {attachmentProblem ?? pickRefusal}
          </p>
        )}
        {hostDocPickerOpen && (
          // `browsedDaemonInstanceId` must name the host `client` enumerates from, because every ref
          // the picker yields is stamped with it. It is `stagingDaemonInstanceId` because both derive
          // from the connected daemon — the host whose documents are listed is the host stamped. It is
          // deliberately NOT the session host: a document is read where it lives, and the session host
          // fetches it from there.
          <HostDocumentPicker
            client={client}
            sessionToken={sessionToken}
            browsedDaemonInstanceId={stagingDaemonInstanceId}
            project={projects.find((p) => p.projectId === effectiveProjectId)}
            onPick={attachHostDocument}
            onClose={closeHostDocPicker}
          />
        )}
      </AttachmentDropZone>

      {/* Error */}
      {error !== null && (
        <p data-testid="create-session-error" className="text-sm text-destructive">
          {error}
        </p>
      )}

      {/* Actions */}
      <div className="flex gap-2 pt-2">
        <Button
          type="button"
          data-testid="create-session-cancel-btn"
          variant="outline"
          onClick={onCancel}
          disabled={submitting}
        >
          Cancel
        </Button>
        <Button
          type="button"
          data-testid="create-session-submit-btn"
          disabled={!isSubmitEnabled}
          onClick={handleSubmit}
        >
          Create session
        </Button>
      </div>

      {/* Branch-conflict prompt — an overlay over this form, which stays mounted with its values so
          cancelling returns the operator to what they typed. */}
      {branchConflict !== null && (
        <BranchConflictDialog
          conflict={branchConflict}
          onSwitchToOwner={() => resolveBranchConflict(branchConflict, { choice: "switch-to-owner" })}
          onAddAgent={() => resolveBranchConflict(branchConflict, { choice: "add-agent" })}
          onRename={(branchName) => {
            // Keep the form's own field in step with the name actually submitted.
            setNewBranchName(branchName);
            resolveBranchConflict(branchConflict, { choice: "rename", branchName });
          }}
          onCancel={() => setBranchConflict(null)}
        />
      )}
    </div>
  );
}

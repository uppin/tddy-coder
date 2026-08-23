import React, { useEffect, useMemo, useState } from "react";
import { flushSync } from "react-dom";
import type { Client } from "@connectrpc/connect";
import type { BranchConflict, ConnectionService, ProjectEntry, SessionEntry, ToolInfo } from "../../gen/connection_pb";
import { localBranchName } from "../../lib/branchNames";
import { projectSelectOptions } from "../../lib/projectSelectOptions";
import { safeTestIdPart } from "../../lib/testId";
import type { BaseBranchOption } from "./prstack/baseBranchChoice";
import {
  startSessionOverridesFor,
  type BranchConflictResolution,
  type BranchFieldOverrides,
  type BranchWorktreeIntent,
} from "../../lib/branchConflict";
import { prStackOrchestrators, stackBaseSessionCandidates } from "../../utils/stackParents";
import { useDaemons, useSelectedDaemon } from "../../rpc/selectedDaemon";
import { useAgentModels } from "../../rpc/useAgentModels";
import {
  useSessionAttachments,
  type SessionAttachmentInit,
  type StartSessionRequestInit,
} from "../../hooks/useSessionAttachments";
import { Button } from "../ui/button";
import { useAvailableAgents } from "./useAvailableAgents";
import { CreateSessionAgentSelect } from "./CreateSessionAgentSelect";
import { inputClass, labelClass } from "./createSessionFormStyles";
import { useSelectableAgents } from "./useSelectableAgents";
import {
  agentForHost,
  hostRunningSession,
  selectableAgentValue,
} from "./selectableAgentOptions";
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
  // The *name* of the agent the operator picked (`claude`, an assistant's name). Which host offers it
  // is `daemonInstanceId`, and the pair is resolved against the fleet's catalog by `selectedAgent`
  // below — so a Host change re-points the same name rather than invalidating the selection.
  const [agent, setAgent] = useState("");
  const [recipe, setRecipe] = useState(initialValues?.recipe ?? "tdd");
  const [stackParent, setStackParent] = useState(initialValues?.stackParent ?? "");
  // The existing session whose branch seeds a new pr-stack orchestrator's stack as its single root
  // node. Empty leaves the stack unseeded, which is what every caller sent before this control
  // existed — the agent then plans it.
  const [prStackBaseSessionId, setPrStackBaseSessionId] = useState("");
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
  const [tools, setTools] = useState<ToolInfo[]>([]);
  // The qualified ids (`name@daemon_instance_id`) of the agents to attach at start. Qualified rather
  // than bare names because the picker lists every host's agents and two hosts routinely offer a def
  // of the same name — a bare name cannot say which of them was picked.
  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>([]);
  const [managedCodebase, setManagedCodebase] = useState(false);
  const [semanticIndex, setSemanticIndex] = useState(false);
  // Which daemon's filesystem holds the worktree. Empty means "same as host" — the co-located
  // placement every session had before docs/ft/daemon/remote-managed-worktree.md.
  const [codebaseDaemonInstanceId, setCodebaseDaemonInstanceId] = useState("");
  /**
   * Whether placing the codebase on another daemon is even on offer.
   *
   * claude-cli only: it is the one agent that can be *prevented* from touching a local filesystem
   * (`--allowedTools`/`--disallowedTools`), so it is the only type the daemon accepts a split for.
   * Never in peer mode — that flow joins an orchestrator's existing worktree, so the placement is
   * settled by the session being joined. And never without a common room to name a host in.
   *
   * This is what the selector renders on. `isSplitCodebase` below builds on it rather than
   * restating it, so the control's visibility and everything a split withdraws cannot drift apart.
   */
  const canChooseCodebaseHost =
    sessionType === "claude-cli" && managedCodebase && !peerMode && daemons.length > 0;

  /**
   * The session's worktree lives on a daemon other than the one running its agent — see
   * docs/ft/daemon/remote-managed-worktree.md.
   *
   * Governs everything a split cannot also ask for: the workflow recipe, the sandbox and the
   * permission bypass all resolve a worktree on the daemon running the agent, which a split session
   * does not have, and the daemon refuses each by name. Specialized agents and the semantic index
   * are not among them — an agent is placeable on any host, and the index is built wherever the
   * worktree is, which on a split session is the codebase host.
   */
  const isSplitCodebase =
    canChooseCodebaseHost &&
    codebaseDaemonInstanceId !== "" &&
    // Naming the session's own host is the explicit spelling of "co-located", and the daemon
    // classifies it exactly that way. Treating it as a split here would withdraw the recipe from a
    // session that is going to run with one. `daemonInstanceId` rather than its peer-mode-aware
    // form: `canChooseCodebaseHost` already means the two are the same value.
    codebaseDaemonInstanceId !== daemonInstanceId;
  // The whole session list as the daemon reported it. Kept raw because two pickers draw different
  // views of it — the orchestrators that can parent this session, and the sessions that own a branch
  // a stack can be seeded from — and one fetch feeds both.
  const [sessions, setSessions] = useState<SessionEntry[]>([]);
  const [remoteBranches, setRemoteBranches] = useState<string[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Set when the daemon refused a creation because another session already owns the requested branch.
  // The form stays mounted behind the prompt, so cancelling returns to it with its values intact.
  const [branchConflict, setBranchConflict] = useState<BranchConflict | null>(null);

  // One option per logical project: aggregated `ListProjects` returns a row per (project, host), and
  // this form's Project selector submits only a project id (the host has its own selector).
  const projectOptions = useMemo(() => projectSelectOptions(projects), [projects]);

  // The orchestrators this session can be stack-parented to.
  const stackParentOptions = useMemo(() => prStackOrchestrators(sessions), [sessions]);

  // In peer mode the project and host are locked to the orchestrating session (the pane reuses its
  // worktree via `repo_path`), so the Project/Host selectors are hidden and submit must send the
  // frozen `initialValues` values — not the live form state, which the mount-time auto-select could
  // have overridden with a different single project.
  const effectiveProjectId = peerMode ? (initialValues?.projectId ?? "") : projectId;
  const effectiveDaemonInstanceId = peerMode
    ? (initialValues?.daemonInstanceId ?? "")
    : daemonInstanceId;

  /**
   * The daemon the browser's RPC reaches: the host the fan-out reads as its home, and the host that
   * serves a request naming none.
   */
  const connectedInstanceId = selectedInstanceId ?? "";

  // Every common-room daemon's agents — the thing a tool session is started *as*. `ListAgents`
  // answers for the responding daemon only, so without this fan-out an assistant created on another
  // host is absent from the form rather than merely hard to find.
  const selectableAgents = useSelectableAgents(client, connectedInstanceId);

  // Whether there is a host to name at all. The same condition the Host select is rendered on: with
  // no common room there is one host, so nothing to disambiguate and nothing to caption.
  const hostsAdvertised = daemons.length > 0;

  /**
   * The host whose agents the session can actually be started as — the host the form will ask for, in
   * the spelling the fan-out stamps its rows with. See `hostRunningSession` for why the empty
   * `daemon_instance_id` the peer flow can carry names a host rather than lacking one. The request is
   * unaffected: it keeps sending `effectiveDaemonInstanceId` exactly as given.
   */
  const agentHostInstanceId = hostRunningSession(effectiveDaemonInstanceId, connectedInstanceId);

  /**
   * The agents this form may offer, and the hosts whose silence it may report: every host's in the
   * standalone flow, and **only the session's host** in peer mode. A peer joins an orchestrator's
   * worktree, so its host is settled before the form opens and the request carries
   * `effectiveDaemonInstanceId` whatever the select shows — offering another host's agent there would
   * compose a pair the host cannot resolve, and another host's outage is not this session's problem.
   */
  const offeredAgents = peerMode
    ? selectableAgents.agents.filter((a) => a.daemonInstanceId === agentHostInstanceId)
    : selectableAgents.agents;
  const offeredHostFailures = peerMode
    ? selectableAgents.failures.filter((f) => f.daemonInstanceId === agentHostInstanceId)
    : selectableAgents.failures;

  /**
   * The agent the session will actually start as: the picked name, as the host taking the session
   * offers it. Derived from `(agent, agentHostInstanceId)` rather than stored beside them, so neither
   * the control nor the request can carry a pair the host does not have — that is what makes a Host
   * change re-point the selection (same name on the new host, else that host's first, else none) and
   * what makes the opening selection the home host's first agent without any effect correcting state
   * after the fact.
   *
   * `agent` therefore holds the *name* the operator last picked, kept across host changes.
   */
  const selectedAgent = agentForHost(offeredAgents, agentHostInstanceId, agent);
  const selectedAgentId = selectedAgent?.id ?? "";
  const selectedAgentValue =
    selectedAgent === null ? "" : selectableAgentValue(selectedAgent, hostsAdvertised);

  // The model catalog is enumerated per selected backend: the chosen agent for tool sessions, and
  // the "claude-cli" pseudo-agent for the Claude CLI session type.
  const modelAgentKey =
    sessionType === "claude-cli"
      ? CLAUDE_CLI_AGENT
      : sessionType === "cursor-cli"
        ? CURSOR_CLI_AGENT
        : selectedAgentId;
  const agentModels = useAgentModels(client, sessionToken, modelAgentKey, daemonInstanceId);

  // Reset the model selection to the backend's advertised default whenever the catalog changes
  // (agent switch, session-type switch). Empty while loading or on a failed probe.
  useEffect(() => {
    setModel(agentModels.defaultModel);
  }, [agentModels.defaultModel]);

  // Every common-room daemon's specialized agents, each labelled with the host that offers it. A
  // host that cannot answer costs one error row rather than the whole picker — see
  // docs/ft/daemon/session-agent-roster.md § Web UI.
  const availableAgents = useAvailableAgents(client, connectedInstanceId);

  const toggleAgent = (agentId: string) => {
    setSelectedAgentIds((prev) =>
      prev.includes(agentId) ? prev.filter((id) => id !== agentId) : [...prev, agentId],
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
        setSessions(resp.sessions as SessionEntry[]);
      })
      .catch(() => {
        // Session list is best-effort; failing to fetch it just leaves the stack-parent and
        // stack-base pickers with nothing to offer.
      });

    // Agents are not read here: they are fanned out across every host by `useSelectableAgents`,
    // since one daemon's answer speaks only for itself.
    Promise.all([client.listProjects({ sessionToken }), client.listTools({})])
      .then(([projectsResp, toolsResp]) => {
        if (cancelled) return;

        const loadedProjects = projectsResp.projects as ProjectEntry[];
        const loadedTools = toolsResp.tools as ToolInfo[];

        setProjects(loadedProjects);
        setTools(loadedTools);

        // Auto-select toolPath.
        if (loadedTools.length > 0) {
          setToolPath(loadedTools[0]!.path);
        }
        // Auto-select projectId when there is exactly one choice — no meaningful decision. Counted
        // in offered options, not rows: a single project carried by two hosts is still one choice.
        const loadedOptions = projectSelectOptions(loadedProjects);
        if (loadedOptions.length === 1) {
          setProjectId(loadedOptions[0]!.projectId);
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

  // The sessions whose branch can seed this orchestrator's stack — scoped to the project and host
  // the form will actually create it on, because a base session in another repository (or on another
  // daemon's checkout) owns a branch this stack cannot base anything off. Derived here rather than
  // beside the parent picker above because it depends on the effective project/host resolved just now.
  const stackBaseSessionOptions = useMemo(
    () =>
      stackBaseSessionCandidates(sessions, {
        projectId: effectiveProjectId,
        daemonInstanceId: effectiveDaemonInstanceId,
      }),
    [sessions, effectiveProjectId, effectiveDaemonInstanceId],
  );

  // A base session belongs to one project on one host, so switching either abandons the choice. Reset
  // it *visibly* — the control returns to "None (agent plans the stack)", which is what submit would
  // now do — instead of leaving a value the picker no longer offers selected behind a blank <select>.
  useEffect(() => {
    setPrStackBaseSessionId("");
  }, [effectiveProjectId, effectiveDaemonInstanceId]);

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
      return Boolean(effectiveProjectId && selectedAgentId && toolPath && model);
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
        // The bare id the host knows it by — the option's value is qualified for the select's sake
        // only, and `daemonInstanceId` already carries the host beside it.
        agent: selectedAgentId,
        recipe,
        stackParent,
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
      sessionType: "claude-cli",
      model,
      permissionMode,
      dangerouslySkipPermissions: isSplitCodebase ? false : dangerouslySkipPermissions,
      initialPrompt,
      sandbox: isSplitCodebase ? false : sandbox,
      managedCodebase,
      // Both ride along on any placement, split or not — an agent reads the codebase through its
      // own placement, and the index is built wherever the worktree is. Only `managedCodebase`
      // gates them: without a managed codebase there is nothing to index and no worktree to give
      // an agent, and the picker and toggle are hidden, so neither value may leak into the request.
      specializedAgents: selectedAgentIds,
      semanticIndex,
      // A remote worktree is reachable only through the mcp__tddy-tools__* proxy that managed
      // codebase installs, so a placement chosen before the toggle was switched off would name a
      // combination the daemon refuses. `isSplitCodebase` also covers peer mode, where the worktree
      // being joined already decides where the codebase lives.
      codebaseDaemonInstanceId: isSplitCodebase ? codebaseDaemonInstanceId : "",
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

  // The specialized-agent multi-select, shared by the cursor-cli and claude-cli managed-codebase
  // blocks. Every host's agents are listed together, so each option names the host that offers it
  // and submits the qualified id — picking "explorer" here cannot silently start another host's
  // agent of the same name. A host that could not be listed is one row; the rest stay on offer.
  const agentPickerSection = (
    <div data-testid="create-session-managed-codebase-section" className="space-y-1">
      {availableAgents.failures.map((failure) => (
        <p
          key={failure.daemonInstanceId}
          data-testid={`create-session-agent-host-error-${safeTestIdPart(failure.daemonInstanceId)}`}
          className="text-sm text-destructive"
        >
          {`${failure.daemonInstanceId}: ${failure.message}`}
        </p>
      ))}
      {availableAgents.agents.length === 0 && availableAgents.failures.length === 0 ? (
        <p className="text-sm text-muted-foreground">No specialized agents available</p>
      ) : (
        availableAgents.agents.map((agent) => (
          <label
            key={agent.agentId}
            className="flex items-center gap-2 text-sm text-muted-foreground"
          >
            <input
              data-testid={`create-session-agent-${safeTestIdPart(agent.agentId)}`}
              type="checkbox"
              className="h-4 w-4 rounded border-input"
              checked={selectedAgentIds.includes(agent.agentId)}
              onChange={() => toggleAgent(agent.agentId)}
            />
            <span>{agent.label || agent.name}</span>
            <span
              data-testid={`create-session-agent-${safeTestIdPart(agent.agentId)}-host`}
              className="text-xs"
            >
              {agent.daemonInstanceId}
            </span>
          </label>
        ))
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
            {projectOptions.map((option) => (
              <option key={option.projectId} value={option.projectId}>
                {option.label}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Tool session fields */}
      {sessionType === "tool" && (
        <>
          <CreateSessionAgentSelect
            agents={offeredAgents}
            failures={offeredHostFailures}
            hostsAdvertised={hostsAdvertised}
            selectedValue={selectedAgentValue}
            onPick={(picked) => {
              setAgent(picked.id);
              // The session runs where its agent is resolvable, so picking one names its host. In
              // peer mode every option already belongs to the frozen host, so this can only ever
              // write that host back.
              setDaemonInstanceId(picked.daemonInstanceId);
            }}
          />

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

          {/* Base the stack on — seeds the new orchestrator's stack with one existing session's
              branch as its single root node, instead of leaving the agent to plan a stack it cannot
              know about. Hangs off the recipe rather than the branch mode: an orchestrator has no
              branch of its own, so there is no branch mode for the control to qualify. Hidden in peer
              mode, where the pane creates a peer on another session's worktree, not an orchestrator. */}
          {recipe === "pr-stack" && !peerMode && (
            <div>
              <label className={labelClass} htmlFor="create-session-pr-stack-base-session">
                Base the stack on
              </label>
              <select
                id="create-session-pr-stack-base-session"
                data-testid="create-session-pr-stack-base-session-select"
                className={inputClass}
                value={prStackBaseSessionId}
                onChange={(e) => setPrStackBaseSessionId(e.target.value)}
              >
                <option value="">None (agent plans the stack)</option>
                {/* Labelled by the branch as well as the id: the branch is what the seeded node is
                    bound to, and what every descendant is based on. */}
                {stackBaseSessionOptions.map((s) => (
                  <option key={s.sessionId} value={s.sessionId}>
                    {`${s.sessionId} — ${s.branch}`}
                  </option>
                ))}
              </select>
            </div>
          )}

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
                  // Closing the section clears what only it could offer, rather than leaving the
                  // values to be stripped at submit: a selection the operator can no longer see is
                  // one the form must no longer hold, and a request that disagrees with the screen
                  // is how a picked agent went missing without an error.
                  if (!e.target.checked) {
                    setSemanticIndex(false);
                    setSelectedAgentIds([]);
                  }
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
                {agentPickerSection}
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

          {/* A split session runs unjailed on this host, and its entire "no route to the local
              filesystem" guarantee rests on the agent's deny list. Whether that list survives
              --dangerously-skip-permissions is not something this repo pins, so the combination is
              withdrawn rather than assumed safe. */}
          {!isSplitCodebase && (
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
          )}

          {/* A sandboxed spawn resolves its worktree on this daemon, which a split session has no
              worktree on — the daemon refuses the pair, so the choice is withdrawn rather than
              offered and then rejected. */}
          {!isSplitCodebase && (
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
          )}

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
                  // Closing the section clears what only it could offer, rather than leaving the
                  // values to be stripped at submit: a selection the operator can no longer see is
                  // one the form must no longer hold, and a request that disagrees with the screen
                  // is how a picked agent went missing without an error.
                  if (!e.target.checked) {
                    setSemanticIndex(false);
                    setSelectedAgentIds([]);
                  }
                }}
              />
              Managed codebase
            </label>
            {managedCodebase && (
              <div className="mt-2 space-y-3 pl-4">
                {/* A recipe's tooling runs against a repository on the daemon hosting the agent, and
                    a split session has none — the daemon refuses the combination. Withdrawing the
                    control is honest about that; leaving it visible would offer a choice whose only
                    effect is to turn a valid placement into a refusal. */}
                {!isSplitCodebase && (
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
                )}
                {/* Codebase host — which daemon's filesystem holds the worktree. Offered only in the
                    claude-cli copy of this block: only claude-cli can be *prevented* from touching a
                    local filesystem (--allowedTools/--disallowedTools), so it is the only session
                    type the daemon accepts a split placement for.
                    See docs/ft/daemon/remote-managed-worktree.md. */}
                {canChooseCodebaseHost && (
                  <div>
                    <label className={labelClass} htmlFor="create-session-codebase-host">
                      Codebase host
                    </label>
                    <select
                      id="create-session-codebase-host"
                      data-testid="create-session-codebase-host-select"
                      className={inputClass}
                      value={codebaseDaemonInstanceId}
                      onChange={(e) => setCodebaseDaemonInstanceId(e.target.value)}
                    >
                      <option value="">Same as host</option>
                      {daemons.map((d) => (
                        <option key={d.instanceId} value={d.instanceId}>
                          {d.label}
                        </option>
                      ))}
                    </select>
                  </div>
                )}
                {/* No split guard: an agent is placeable on any host, and the placement only
                    decides how it reads the codebase — an agent on the codebase host reads that
                    worktree directly, one anywhere else reads a clone the session's worktree sync
                    keeps current. So the picker offers the same roster either way. */}
                {agentPickerSection}
                {/* No split guard: the index is built wherever the worktree is, which on a split
                    session is the codebase host. */}
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
      {stackParentOptions.length > 0 && !peerMode && (
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
            {stackParentOptions.map((s) => (
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

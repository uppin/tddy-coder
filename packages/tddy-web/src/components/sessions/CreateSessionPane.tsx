import React, { useEffect, useMemo, useState } from "react";
import { flushSync } from "react-dom";
import type { BranchConflict } from "../../gen/session_pb";
import { projectSelectOptions } from "../../lib/projectSelectOptions";
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
import { useAvailableAgents } from "./useAvailableAgents";
import { sshConfigListDaemonId } from "./CreateSessionSshConfigSelect";
import { inputClass, labelClass } from "./createSessionFormStyles";
import { WORKFLOW_RECIPES } from "./createSessionRecipes";
import { buildStartSessionRequest, type SessionType } from "./createSessionRequest";
import { CreateSessionTypeToggle } from "./CreateSessionTypeToggle";
import { CreateSessionModelField } from "./CreateSessionModelField";
import { CreateSessionAgentPickerSection } from "./CreateSessionAgentPickerSection";
import { CreateSessionToolFields } from "./CreateSessionToolFields";
import { CreateSessionCursorCliFields } from "./CreateSessionCursorCliFields";
import { CreateSessionSandboxedCodebaseToggle } from "./CreateSessionSandboxedCodebaseToggle";
import { CreateSessionManagedCodebaseFields } from "./CreateSessionManagedCodebaseFields";
import { CreateSessionBranchFields } from "./CreateSessionBranchFields";
import { CreateSessionHostAndProjectFields } from "./CreateSessionHostAndProjectFields";
import { CreateSessionAttachmentsSection } from "./CreateSessionAttachmentsSection";
import { CreateSessionPermissionFields } from "./CreateSessionPermissionFields";
import { CreateSessionActions } from "./CreateSessionActions";
import { CreateSessionStackParentSelect } from "./CreateSessionStackParentSelect";
import { useProjectBranches } from "./useProjectBranches";
import { useCreateSessionCatalogs } from "./useCreateSessionCatalogs";
import type { CreateSessionPaneProps } from "./createSessionPaneProps";
import {
  placementAfterToggling,
  sandboxedCodebaseUnavailability,
  type CodebasePlacementChoice,
} from "./codebasePlacement";
import { useSelectableAgents } from "./useSelectableAgents";
import {
  agentForHost,
  hostRunningSession,
  selectableAgentValue,
} from "./selectableAgentOptions";
import { BranchConflictDialog } from "./BranchConflictDialog";

/** Pseudo-agent key used to fetch the claude-cli session type's model catalog. */
const CLAUDE_CLI_AGENT = "claude-cli";
const CURSOR_CLI_AGENT = "cursor-cli";

type BranchIntent = BranchWorktreeIntent;

export type {
  CreateSessionInitialValues,
  CreateSessionPaneProps,
} from "./createSessionPaneProps";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function CreateSessionPane({
  client,
  projectClient,
  catalogClient,
  sessionFilesClient,
  worktreeClient,
  sessionToken,
  onCancel,
  onCreated,
  initialValues,
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
  // The planned node the caller opened this form for. Not state of its own: the operator picks a
  // stack parent here, never a node — so the id is derived from whether the parent is still the one
  // the node belongs to, and dropped the moment it is not.
  //
  // A node id names a node in exactly one orchestrator's plan. Sent beside a different parent, the
  // daemon's `LinkStackNode` answers `not_found` and — per D36, so a failed link never fails a spawn
  // that already created the worktree — only logs it: the node would stay branchless and childless,
  // which is the very bug the id exists to fix, with nothing said to the operator. An empty id falls
  // the daemon back to its branch-derived local lookup, which is correct for a same-host spawn and is
  // what every spawn used before the id existed (D34).
  const stackNodeId =
    stackParent === (initialValues?.stackParent ?? "") ? (initialValues?.stackNodeId ?? "") : "";
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

  const { projects, sessions } = useCreateSessionCatalogs({
    client,
    projectClient,
    catalogClient,
    sessionToken,
    setToolPath,
    setProjectId,
  });

  // The qualified ids (`name@daemon_instance_id`) of the agents to attach at start. Qualified rather
  // than bare names because the picker lists every host's agents and two hosts routinely offer a def
  // of the same name — a bare name cannot say which of them was picked.
  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>([]);
  const [managedCodebase, setManagedCodebase] = useState(false);
  // The inverted placement: this session's own checkout inside a `--workspace-tools` jail, with the
  // agent beside it, unconfined and stripped of its native filesystem and shell tools.
  // See docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md.
  const [sandboxedCodebase, setSandboxedCodebase] = useState(false);
  const [semanticIndex, setSemanticIndex] = useState(false);
  // Which daemon's filesystem holds the worktree. Empty means "same as host" — the co-located
  // placement every session had before docs/ft/daemon/remote-managed-worktree.md.
  const [codebaseDaemonInstanceId, setCodebaseDaemonInstanceId] = useState("");
  // OpenSSH Host alias the exec catalog runs on. Empty is LocalShell on the code-managing host.
  const [sshConfigHost, setSshConfigHost] = useState("");
  /**
   * Whether placing the codebase on another daemon is even on offer.
   *
   * claude-cli only: it is the one agent that can be *prevented* from touching a local filesystem
   * (`--allowedTools`/`--disallowedTools`), so it is the only type the daemon accepts a split for.
   * And never without a common room to name a host in.
   *
   * This is what the selector renders on. `isSplitCodebase` below builds on it rather than
   * restating it, so the control's visibility and everything a split withdraws cannot drift apart.
   */
  const canChooseCodebaseHost =
    sessionType === "claude-cli" && managedCodebase && daemons.length > 0;

  /**
   * The session's worktree lives on a daemon other than the one running its agent — see
   * docs/ft/daemon/remote-managed-worktree.md.
   *
   * Governs everything a split cannot also ask for: the workflow recipe and the permission bypass
   * both resolve a worktree on the daemon running the agent, which a split session does not have,
   * and the daemon refuses each by name. The sandbox is not among them — on a split placement it
   * confines the codebase host (the jail runs where the checkout is), so the combination is
   * admitted. Specialized agents and the semantic index are not among them either — an agent is
   * placeable on any host, and the index is built wherever the worktree is, which on a split
   * session is the codebase host.
   */
  const isSplitCodebase =
    canChooseCodebaseHost &&
    codebaseDaemonInstanceId !== "" &&
    // Naming the session's own host is the explicit spelling of "co-located", and the daemon
    // classifies it exactly that way. Treating it as a split here would withdraw the recipe from a
    // session that is going to run with one.
    codebaseDaemonInstanceId !== daemonInstanceId;
  // The whole session list as the daemon reported it. Kept raw because two pickers draw different
  // views of it — the orchestrators that can parent this session, and the sessions that own a branch
  // a stack can be seeded from — and one fetch feeds both.
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

  // The host that owns the selected stack parent. `stackParent` is a bare session id and the base
  // it resolves to is read out of that session's own changeset, so the daemon serving this start
  // has to be told which host holds it: the picker is fed by a fanned-out, all-host `ListSessions`
  // and routinely offers an orchestrator living on a different daemon than the child is started on.
  // Empty when the list holds no such session — the wire default, meaning the serving daemon's own
  // sessions tree, which is the only place it could look anyway.
  const stackParentDaemonInstanceId = useMemo(
    () => sessions.find((s) => s.sessionId === stackParent)?.daemonInstanceId ?? "",
    [sessions, stackParent],
  );

  /**
   * The daemon the browser's RPC reaches: the host the fan-out reads as its home, and the host that
   * serves a request naming none.
   */
  const connectedInstanceId = selectedInstanceId ?? "";

  // Every common-room daemon's agents — the thing a tool session is started *as*. `ListAgents`
  // answers for the responding daemon only, so without this fan-out an assistant created on another
  // host is absent from the form rather than merely hard to find.
  const selectableAgents = useSelectableAgents(catalogClient, connectedInstanceId);

  // Whether there is a host to name at all. The same condition the Host select is rendered on: with
  // no common room there is one host, so nothing to disambiguate and nothing to caption.
  const hostsAdvertised = daemons.length > 0;

  /**
   * The host whose agents the session can actually be started as — the host the form will ask for, in
   * the spelling the fan-out stamps its rows with. See `hostRunningSession` for why an empty
   * `daemon_instance_id` names a host rather than lacking one. The request is unaffected: it keeps
   * sending `daemonInstanceId` exactly as the form holds it.
   */
  const agentHostInstanceId = hostRunningSession(daemonInstanceId, connectedInstanceId);

  /** The directory's entry for the host running this session, or `null` when it names none. */
  const sessionHost = daemons.find((d) => d.instanceId === agentHostInstanceId) ?? null;

  /**
   * Why that host cannot jail this session's checkout, or `null` when it can.
   *
   * Read off the host's own advertisement rather than guessed from a platform: a daemon too old to
   * advertise the capability would answer the request field by starting an ordinary, unconfined
   * session. A host the directory does not name at all is no advertisement either.
   */
  const sandboxedCodebaseUnavailableReason = sandboxedCodebaseUnavailability(sessionHost);

  /** What that host's jail leaves unconfined, when it says so. Absent = nothing to caveat. */
  const jailSharesTheFilesystemRoot =
    sessionHost?.sandboxedCodebase?.confinesFilesystem === false;

  // A placement the selected host cannot serve is one the form must not hold: the host is pickable
  // after the placement is, and a request carrying a choice the disabled control would never have
  // allowed is how a session comes back unconfined with nothing said.
  useEffect(() => {
    if (sandboxedCodebaseUnavailableReason !== null) setSandboxedCodebase(false);
  }, [sandboxedCodebaseUnavailableReason]);

  /**
   * The placement the form currently holds, as the rules module names it.
   *
   * `Sandbox` and `Managed codebase` coexist — on a split placement the sandbox confines the
   * codebase host, so the daemon admits the pair — and either of them means the codebase is not
   * jailed, which is all the rule needs from them. Their order here is therefore immaterial.
   */
  const currentPlacement: CodebasePlacementChoice = sandboxedCodebase
    ? "sandboxedCodebase"
    : managedCodebase
      ? "managed"
      : sandbox
        ? "sandbox"
        : "none";

  /**
   * Apply the placement rule for the control the operator just toggled.
   *
   * Only the jailed codebase is exclusive with *both* of the others, so this owns exactly that
   * axis: the two older toggles keep their own state, and choosing either of them clears the jail.
   * Choosing the jail clears them, together with everything only the managed section could offer —
   * a selection the operator can no longer see is one the form must no longer hold.
   */
  /**
   * Whether this placement withdraws `--dangerously-skip-permissions`.
   *
   * Both placements that withdraw it confine the agent the same way — by taking its native
   * filesystem and shell tools off its argv — so on both, the deny list *is* the confinement.
   * Whether that list survives the bypass flag is not something this repo pins, so the control is
   * withheld rather than the combination assumed safe, and the daemon refuses each by name. A
   * split session withdraws it because it has no local worktree for the flag to be about; a
   * jailed codebase withdraws it because the flag would be about the one thing keeping the agent
   * out of the host's filesystem.
   * See docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md
   * § What's staying the same.
   */
  const placementWithdrawsPermissionBypass = isSplitCodebase || sandboxedCodebase;

  const applyPlacement = (toggled: CodebasePlacementChoice, on: boolean) => {
    const next = placementAfterToggling(currentPlacement, toggled, on);
    setSandboxedCodebase(next === "sandboxedCodebase");
    if (next === "sandboxedCodebase") {
      setSandbox(false);
      setManagedCodebase(false);
      setSemanticIndex(false);
      setSelectedAgentIds([]);
      setCodebaseDaemonInstanceId("");
    }
  };

  /**
   * Host whose `~/.ssh/config` the SSH dropdown lists. Co-located: the session host. Split: the
   * codebase host — that daemon is the OpenSSH client (n4).
   */
  const sshListDaemonId = sshConfigListDaemonId(
    agentHostInstanceId,
    codebaseDaemonInstanceId,
  );

  useEffect(() => {
    setSshConfigHost("");
  }, [sshListDaemonId]);

  /** The agents this form may offer, and the hosts whose silence it may report. */
  const offeredAgents = selectableAgents.agents;
  const offeredHostFailures = selectableAgents.failures;

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
  const agentModels = useAgentModels(catalogClient, sessionToken, modelAgentKey, daemonInstanceId);

  // Reset the model selection to the backend's advertised default whenever the catalog changes
  // (agent switch, session-type switch). Empty while loading or on a failed probe.
  useEffect(() => {
    setModel(agentModels.defaultModel);
  }, [agentModels.defaultModel]);

  // Every common-room daemon's specialized agents, each labelled with the host that offers it. A
  // host that cannot answer costs one error row rather than the whole picker — see
  // docs/ft/daemon/session-agent-roster.md § Web UI.
  const availableAgents = useAvailableAgents(catalogClient, connectedInstanceId);

  const toggleAgent = (agentId: string) => {
    setSelectedAgentIds((prev) =>
      prev.includes(agentId) ? prev.filter((id) => id !== agentId) : [...prev, agentId],
    );
  };

  const remoteBranches = useProjectBranches({
    client,
    projectClient,
    sessionToken,
    projectId,
    branchIntent,
    daemonInstanceId,
    preFilledBranchToWorkOn,
    setSelectedBranchToWorkOn,
  });

  // The sessions whose branch can seed this orchestrator's stack — scoped to the project and host
  // the form will actually create it on, because a base session in another repository (or on another
  // daemon's checkout) owns a branch this stack cannot base anything off. Derived here rather than
  // beside the parent picker above because it depends on the project/host the form currently holds.
  const stackBaseSessionOptions = useMemo(
    () => stackBaseSessionCandidates(sessions, { projectId, daemonInstanceId }),
    [sessions, projectId, daemonInstanceId],
  );

  // A base session belongs to one project on one host, so switching either abandons the choice. Reset
  // it *visibly* — the control returns to "None (agent plans the stack)", which is what submit would
  // now do — instead of leaving a value the picker no longer offers selected behind a blank <select>.
  useEffect(() => {
    setPrStackBaseSessionId("");
  }, [projectId, daemonInstanceId]);

  // The attach rows and everything that follows from them: the effective size cap, the refusal shown
  // next to a bad row, the upload of local files on submit, and the streamed start that reports the
  // host's materialization progress.
  const sessionAttachments = useSessionAttachments({
    client,
    sessionFilesClient,
    sessionToken,
    sessionDaemonInstanceId: daemonInstanceId,
    initialAttachments: initialValues?.attachments,
  });
  // The four the pane itself reads; the rest are the attachments section's, which takes the whole
  // hook result rather than fifteen props.
  const {
    problem: attachmentProblem,
    resetProgress: resetAttachmentProgress,
    stageAttachments,
    startSessionStreamed,
  } = sessionAttachments;

  const isSubmitEnabled = (() => {
    if (submitting) return false;
    // An attachment the daemon would refuse (duplicate or unsafe basename) fails the whole creation,
    // so it is refused in the form instead.
    if (attachmentProblem !== null) return false;
    // A model is always required and comes from the daemon-advertised catalog; a failed/loading
    // probe leaves `model` empty, which disables Create (no fallback).
    if (sessionType === "tool") {
      return Boolean(projectId && selectedAgentId && toolPath && model);
    }
    return Boolean(projectId && model);
  })();

  /** Build one `StartSession` request for the current form state. */
  const startSessionRequest = (
    branchOverrides: BranchFieldOverrides | null,
    requestAttachments: SessionAttachmentInit[],
  ): StartSessionRequestInit =>
    buildStartSessionRequest(
      {
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
      },
      branchOverrides,
      requestAttachments,
    );

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

  const modelField = (
    <CreateSessionModelField agentModels={agentModels} model={model} setModel={setModel} />
  );

  const agentPickerSection = (
    <CreateSessionAgentPickerSection
      availableAgents={availableAgents}
      selectedAgentIds={selectedAgentIds}
      toggleAgent={toggleAgent}
    />
  );

  return (
    <div
      data-testid="create-session-pane"
      className="flex flex-col h-full overflow-y-auto p-4 space-y-4"
    >
      <h2 className="text-sm font-semibold">New session</h2>

      {/* Session type toggle */}
      <CreateSessionTypeToggle sessionType={sessionType} setSessionType={setSessionType} />

      <CreateSessionHostAndProjectFields
        daemons={daemons}
        daemonInstanceId={daemonInstanceId}
        setDaemonInstanceId={setDaemonInstanceId}
        projects={projects}
        projectOptions={projectOptions}
        projectId={projectId}
        setProjectId={setProjectId}
      />

      {/* Tool session fields */}
      {sessionType === "tool" && (
        <CreateSessionToolFields
          offeredAgents={offeredAgents}
          offeredHostFailures={offeredHostFailures}
          hostsAdvertised={hostsAdvertised}
          selectedAgentValue={selectedAgentValue}
          setAgent={setAgent}
          setDaemonInstanceId={setDaemonInstanceId}
          recipe={recipe}
          setRecipe={setRecipe}
          prStackBaseSessionId={prStackBaseSessionId}
          setPrStackBaseSessionId={setPrStackBaseSessionId}
          stackBaseSessionOptions={stackBaseSessionOptions}
          modelField={modelField}
        />
      )}

      {/* Cursor CLI session fields */}
      {sessionType === "cursor-cli" && (
        <CreateSessionCursorCliFields
          modelField={modelField}
          agentPickerSection={agentPickerSection}
          sandbox={sandbox}
          setSandbox={setSandbox}
          initialPrompt={initialPrompt}
          setInitialPrompt={setInitialPrompt}
          managedCodebase={managedCodebase}
          setManagedCodebase={setManagedCodebase}
          setSemanticIndex={setSemanticIndex}
          setSelectedAgentIds={setSelectedAgentIds}
          recipe={recipe}
          setRecipe={setRecipe}
          semanticIndex={semanticIndex}
        />
      )}

      {/* Claude CLI session fields */}
      {sessionType === "claude-cli" && (
        <>
          {modelField}

          <CreateSessionPermissionFields
            permissionMode={permissionMode}
            setPermissionMode={setPermissionMode}
            dangerouslySkipPermissions={dangerouslySkipPermissions}
            setDangerouslySkipPermissions={setDangerouslySkipPermissions}
            placementWithdrawsPermissionBypass={placementWithdrawsPermissionBypass}
            sandbox={sandbox}
            setSandbox={setSandbox}
            applyPlacement={applyPlacement}
          />

          <CreateSessionSandboxedCodebaseToggle
            sandboxedCodebase={sandboxedCodebase}
            sandboxedCodebaseUnavailableReason={sandboxedCodebaseUnavailableReason}
            jailSharesTheFilesystemRoot={jailSharesTheFilesystemRoot}
            applyPlacement={applyPlacement}
          />

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
                  applyPlacement("managed", e.target.checked);
                }}
              />
              Managed codebase
            </label>
            {managedCodebase && (
              <CreateSessionManagedCodebaseFields
                isSplitCodebase={isSplitCodebase}
                recipe={recipe}
                setRecipe={setRecipe}
                canChooseCodebaseHost={canChooseCodebaseHost}
                codebaseDaemonInstanceId={codebaseDaemonInstanceId}
                setCodebaseDaemonInstanceId={setCodebaseDaemonInstanceId}
                daemons={daemons}
                sessionType={sessionType}
                sshListDaemonId={sshListDaemonId}
                sessionToken={sessionToken}
                sshConfigHost={sshConfigHost}
                setSshConfigHost={setSshConfigHost}
                agentPickerSection={agentPickerSection}
                semanticIndex={semanticIndex}
                setSemanticIndex={setSemanticIndex}
              />
            )}
          </div>
        </>
      )}

      <CreateSessionStackParentSelect
        stackParentOptions={stackParentOptions}
        stackParent={stackParent}
        setStackParent={setStackParent}
      />

      <CreateSessionBranchFields
        branchIntent={branchIntent}
        setBranchIntent={setBranchIntent}
        baseBranchLabel={initialValues?.baseBranchLabel}
        initialStackParent={initialValues?.stackParent}
        baseBranchOptions={baseBranchOptions}
        selectedBaseBranch={selectedBaseBranch}
        setSelectedBaseBranch={setSelectedBaseBranch}
        newBranchName={newBranchName}
        setNewBranchName={setNewBranchName}
        sessionType={sessionType}
        createRemoteBranch={createRemoteBranch}
        setCreateRemoteBranch={setCreateRemoteBranch}
        remoteBranches={remoteBranches}
        selectedBranchToWorkOn={selectedBranchToWorkOn}
        setSelectedBranchToWorkOn={setSelectedBranchToWorkOn}
      />

      <CreateSessionAttachmentsSection
        sessionAttachments={sessionAttachments}
        client={client}
        sessionFilesClient={sessionFilesClient}
        worktreeClient={worktreeClient}
        sessionToken={sessionToken}
        submitting={submitting}
        projects={projects}
        projectId={projectId}
      />

      <CreateSessionActions
        error={error}
        submitting={submitting}
        isSubmitEnabled={isSubmitEnabled}
        onCancel={onCancel}
        handleSubmit={handleSubmit}
      />

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

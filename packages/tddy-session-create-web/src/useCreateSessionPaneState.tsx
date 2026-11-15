import React, { useMemo, useState } from "react";
import { CreateSessionRunInTmuxToggle } from "./CreateSessionRunInTmuxToggle";
import {
  registerSessionTypeOptions,
  sessionLaunchOf,
  sessionTypeOptions,
  TDDY_CODER_LAUNCH,
} from "./cliHarnessOptions";
import {
  agentForSessionType,
  allowsPlacement,
  bindsParam,
  cliAgentFeaturesOf,
  hasCapability,
} from "./codingAgentMetadata";
import { projectSelectOptions } from "./lib/projectSelectOptions";
import type { BaseBranchOption } from "./baseBranchOptions";
import type { BranchWorktreeIntent } from "./lib/branchConflict";
import { useSessionAttachments } from "./hooks/useSessionAttachments";
import { useAvailableAgents } from "./useAvailableAgents";
import type { CreateSessionFormValues, SessionType } from "./createSessionRequest";
import { whyMainCannotRun } from "./projectHomeSessionTypes";
import { useProjectHomeStartSubmit } from "./useProjectHomeStartSubmit";
import { CreateSessionTypeToggle } from "./CreateSessionTypeToggle";
import { CreateSessionModelField } from "./CreateSessionModelField";
import { BUILT_IN_PROVIDER, CreateSessionProviderField } from "./CreateSessionProviderField";
import { CreateSessionAgentPickerSection } from "./CreateSessionAgentPickerSection";
import { useSpecializedAgentPicks } from "./useSpecializedAgentPicks";
import { CreateSessionToolFields } from "./CreateSessionToolFields";
import { CreateSessionCliFields } from "./CreateSessionCliFields";
import { CreateSessionBranchFields } from "./CreateSessionBranchFields";
import { CreateSessionHostAndProjectFields } from "./CreateSessionHostAndProjectFields";
import { CreateSessionAttachmentsSection } from "./CreateSessionAttachmentsSection";
import { CreateSessionActions } from "./CreateSessionActions";
import { CreateSessionStartProgress } from "./CreateSessionStartProgress";
import { CreateSessionStackParentSelect } from "./CreateSessionStackParentSelect";
import { useProjectBranches } from "./useProjectBranches";
import { useCreateSessionCatalogs } from "./useCreateSessionCatalogs";
import { useCreateSessionPlacement } from "./useCreateSessionPlacement";
import { useCreateSessionModel } from "./useCreateSessionModel";
import { useCreateSessionEffort } from "./useCreateSessionEffort";
import { useCreateSessionSubmit } from "./useCreateSessionSubmit";
import { useCreateSessionStack } from "./useCreateSessionStack";
import type { CreateSessionPaneProps } from "./createSessionPaneProps";
import { useCreateSessionAgentSelection } from "./useCreateSessionAgentSelection";
import { BranchConflictDialog } from "./BranchConflictDialog";
import { StartPassphrasePrompt } from "./StartPassphrasePrompt";
import { useCreateSessionOrchestrator } from "./useCreateSessionOrchestrator";
import { useStartingHost } from "./useStartingHost";
import { CreateSessionLaunchFields } from "./CreateSessionLaunchFields";
import { useOpenOnMainSessionType } from "./useOpenOnMainSessionType";
import { useDaemons, useSelectedDaemon } from "tddy-client-web";

type BranchIntent = BranchWorktreeIntent;

export type {
  CreateSessionInitialValues,
  CreateSessionPaneProps,
} from "./createSessionPaneProps";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function useCreateSessionPaneState(props: CreateSessionPaneProps) {
  const {
    accountsClient,
    catalogClient,
    client,
    initialValues,
    onCancel,
    onCreated,
    projectClient,
    projectHome,
    sessionFilesClient,
    sessionToken,
    taskClient,
    worktreeClient,
  } = props;
  const daemons = useDaemons();
  const { selectedInstanceId } = useSelectedDaemon();

  // The type the operator chose (or the caller pre-filled). Until one is chosen the form shows its
  // default, derived below from the session types the daemon declared. The main variant fixes the
  // project and host, and opens on the one type that can run main.
  const [chosenSessionType, setSessionType] = useState<SessionType>(
    projectHome ? "" : (initialValues?.sessionType ?? ""),
  );
  const [projectId, setProjectId] = useState(projectHome?.projectId ?? initialValues?.projectId ?? "");
  // The *name* of the agent the operator picked (`claude`, an assistant's name). Which host offers it
  // is `daemonInstanceId`, and the pair is resolved against the fleet's catalog by `selectedAgent`
  // below — so a Host change re-points the same name rather than invalidating the selection.
  const [agent, setAgent] = useState("");
  const [recipe, setRecipe] = useState(initialValues?.recipe ?? "tdd");
  const [toolPath, setToolPath] = useState("");
  const [permissionMode, setPermissionMode] = useState(initialValues?.permissionMode ?? "auto");
  const [dangerouslySkipPermissions, setDangerouslySkipPermissions] = useState(
    initialValues?.dangerouslySkipPermissions ?? false,
  );
  const [initialPrompt, setInitialPrompt] = useState(initialValues?.initialPrompt ?? "");
  // The session's optional display name, as typed; the request builder trims it.
  const [displayName, setDisplayName] = useState("");
  const [branchIntent, setBranchIntent] = useState<BranchIntent>(
    initialValues?.branchIntent ?? "new_branch_from_base",
  );
  const [newBranchName, setNewBranchName] = useState(initialValues?.newBranchName ?? "");
  const [createRemoteBranch, setCreateRemoteBranch] = useState(
    initialValues?.createRemoteBranch ?? false,
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
    projectHome?.daemonInstanceId || initialValues?.daemonInstanceId || selectedInstanceId || "",
  );

  // The whole session list as the daemon reported it. Kept raw because two pickers draw different
  // views of it — the orchestrators that can parent this session, and the sessions that own a branch
  // a stack can be seeded from — and one fetch feeds both.
  // A failed read is reported beside the picker it emptied, apart from the submit `error` below.
  const { projects, sessions, projectsError, toolsError, sessionsError } = useCreateSessionCatalogs({
    client,
    projectClient,
    catalogClient,
    sessionToken,
    setToolPath,
    setProjectId,
  });

  // The qualified ids (`name@daemon_instance_id`) of the agents to attach at start, and placements.
  const agentPicks = useSpecializedAgentPicks();
  const [semanticIndex, setSemanticIndex] = useState(false);

  // One option per logical project: aggregated `ListProjects` returns a row per (project, host), and
  // this form's Project selector submits only a project id (the host has its own selector).
  const projectOptions = useMemo(() => projectSelectOptions(projects), [projects]);

  /**
   * The daemon the browser's RPC reaches: the host the fan-out reads as its home, and the host that
   * serves a request naming none.
   */
  const connectedInstanceId = selectedInstanceId ?? "";

  // `agent` holds the *name* the operator last picked; which agent that is on the host taking the
  // session is derived, never stored (see `useCreateSessionAgentSelection`).
  const {
    offeredAgents,
    offeredSessionTypes,
    offeredHostFailures,
    hostsAdvertised,
    agentHostInstanceId,
    selectedAgentId,
    selectedAgentRouteProviders,
    selectedAgentValue,
  } = useCreateSessionAgentSelection({
    catalogClient,
    connectedInstanceId,
    daemons,
    daemonInstanceId,
    agent,
  });

  // The session types this form offers and the fields it shows follow what the target daemon
  // declared — its session types with their launch kinds, and its agents' capabilities and argv
  // params — never a list of the bundle's own.
  const typeOptions = sessionTypeOptions(
    offeredSessionTypes,
    daemonInstanceId,
    connectedInstanceId,
  );
  registerSessionTypeOptions(typeOptions);
  // Until a type is chosen the form opens on the daemon's first offered type: one that runs the
  // tddy-coder workflow tool when the daemon declares one (`sessionTypeOptions` puts them first).
  // The main variant opens on the type that can run main instead (below).
  const sessionType: SessionType =
    chosenSessionType || (projectHome ? "" : (typeOptions[0]?.id ?? ""));
  const sessionLaunch = sessionLaunchOf(sessionType, typeOptions);
  const sessionAgent = agentForSessionType(
    offeredAgents,
    daemonInstanceId,
    connectedInstanceId,
    sessionType,
  );

  useOpenOnMainSessionType({
    isMainVariant: projectHome !== undefined,
    typeOptions,
    offeredAgents,
    daemonInstanceId,
    connectedInstanceId,
    sessionType,
    setSessionType,
  });

  const { asOrchestrator, orchestratorField } = useCreateSessionOrchestrator(
    sessionAgent,
    projectHome,
  );

  const {
    sandbox,
    setSandbox,
    runInTmux,
    setRunInTmux,
    runInTmuxUnavailable,
    managedCodebase,
    setManagedCodebase,
    sandboxedCodebase,
    codebaseDaemonInstanceId,
    setCodebaseDaemonInstanceId,
    sshConfigHost,
    setSshConfigHost,
    canChooseCodebaseHost,
    isSplitCodebase,
    sandboxedCodebaseUnavailableReason,
    jailSharesTheFilesystemRoot,
    placementWithdrawsPermissionBypass,
    applyPlacement,
    sshListDaemonId,
  } = useCreateSessionPlacement({
    sessionType,
    agentCanSplit: hasCapability(sessionAgent, "split_host"),
    agentOffersJail: allowsPlacement(sessionAgent, "jail"),
    daemons,
    daemonInstanceId,
    agentHostInstanceId,
  });

  const {
    model,
    setModel,
    providerChoice,
    setProviderChoice,
    providerHarness,
    providerRoutes,
    providerAccountId,
    agentModels,
  } = useCreateSessionModel({
    catalogClient,
    accountsClient,
    sessionToken,
    sessionType,
    sessionLaunch,
    selectedAgentId,
    daemonInstanceId,
    selectedAgentRouteProviders,
    cliRouteProviders: sessionAgent?.routeProviders,
    initialModel: initialValues?.model,
  });

  const { effort, effortField } = useCreateSessionEffort(sessionAgent);

  // Every common-room daemon's specialized agents, each labelled with the host that offers it. A
  // host that cannot answer costs one error row rather than the whole picker — see
  // docs/ft/daemon/session-agent-roster.md § Web UI.
  const availableAgents = useAvailableAgents(catalogClient, connectedInstanceId);

  const { remoteBranches, error: remoteBranchesError } = useProjectBranches({
    client,
    projectClient,
    sessionToken,
    projectId,
    branchIntent,
    daemonInstanceId,
    preFilledBranchToWorkOn,
    setSelectedBranchToWorkOn,
  });

  const {
    stackParent,
    setStackParent,
    stackNodeId,
    prStackBaseSessionId,
    setPrStackBaseSessionId,
    stackParentOptions,
    stackParentDaemonInstanceId,
    stackBaseSessionOptions,
  } = useCreateSessionStack({
    sessions,
    projectId,
    daemonInstanceId,
    initialStackParent: initialValues?.stackParent,
    initialStackNodeId: initialValues?.stackNodeId,
  });

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
  // The five the pane itself reads; the rest are the attachments section's, which takes the whole
  // hook result rather than fifteen props.
  const {
    problem: attachmentProblem,
    resetProgress: resetAttachmentProgress,
    stageAttachments,
    startSessionStreamed,
    startProgress,
  } = sessionAttachments;

  const { startingHostId, startingHostLabel } = useStartingHost({
    isSplitCodebase,
    codebaseDaemonInstanceId,
    agentHostInstanceId,
    daemons,
  });

  const form: CreateSessionFormValues = {
    sessionToken,
    projectId,
    branchIntent,
    newBranchName,
    createRemoteBranch,
    selectedBaseBranch,
    selectedBranchToWorkOn,
    daemonInstanceId,
    sessionType,
    sessionLaunch,
    cliFeatures: cliAgentFeaturesOf(sessionAgent, providerHarness !== null),
    toolPath,
    selectedAgentId,
    recipe,
    stackParent,
    stackNodeId,
    stackParentDaemonInstanceId,
    prStackBaseSessionId,
    model,
    effort,
    providerAccountId,
    providerBuiltIn: providerChoice === BUILT_IN_PROVIDER,
    permissionMode,
    dangerouslySkipPermissions,
    placementWithdrawsPermissionBypass,
    initialPrompt,
    sandbox,
    runInTmux,
    managedCodebase,
    sandboxedCodebase,
    isSplitCodebase,
    selectedAgentIds: agentPicks.selectedAgentIds,
    clonePlacedAgentIds: agentPicks.clonePlacedAgentIds,
    semanticIndex,
    codebaseDaemonInstanceId,
    sshConfigHost,
    orchestrator: asOrchestrator,
    displayName,
  };
  const created = useCreateSessionSubmit({
    form,
    attachmentProblem,
    resetAttachmentProgress,
    stageAttachments,
    startSessionStreamed,
    onCreated,
    taskClient,
    sessionToken,
  });
  const { branchConflict, setBranchConflict, resolveBranchConflict } = created;
  const { submitting, error, isSubmitEnabled, handleSubmit } =
    useProjectHomeStartSubmit(projectHome, form, created);

  const modelField = (
    <CreateSessionModelField agentModels={agentModels} model={model} setModel={setModel} />
  );

  const agentPickerSection = (
    <CreateSessionAgentPickerSection
      availableAgents={availableAgents}
      selectedAgentIds={agentPicks.selectedAgentIds}
      toggleAgent={agentPicks.toggleAgent}
      clonePlacedAgentIds={agentPicks.clonePlacedAgentIds}
      setAgentPlacement={agentPicks.setAgentPlacement}
    />
  );

  // One "Run the agent in tmux" control for every CLI harness type: claude-cli places it in its own
  // fields, the other harnesses in the block below.
  const runInTmuxToggle = (
    <CreateSessionRunInTmuxToggle
      runInTmux={runInTmux}
      setRunInTmux={setRunInTmux}
      unavailable={runInTmuxUnavailable}
    />
  );

  return {
    agent,
    agentPickerSection,
    agentPicks,
    applyPlacement,
    asOrchestrator,
    branchConflict,
    branchIntent,
    canChooseCodebaseHost,
    codebaseDaemonInstanceId,
    connectedInstanceId,
    createRemoteBranch,
    created,
    daemonInstanceId,
    daemons,
    dangerouslySkipPermissions,
    displayName,
    effortField,
    error,
    handleSubmit,
    hostsAdvertised,
    initialPrompt,
    isSplitCodebase,
    isSubmitEnabled,
    jailSharesTheFilesystemRoot,
    managedCodebase,
    model,
    modelField,
    newBranchName,
    offeredAgents,
    offeredHostFailures,
    orchestratorField,
    permissionMode,
    placementWithdrawsPermissionBypass,
    prStackBaseSessionId,
    projectId,
    projectOptions,
    projects,
    projectsError,
    providerChoice,
    providerHarness,
    providerRoutes,
    recipe,
    remoteBranches,
    remoteBranchesError,
    resolveBranchConflict,
    runInTmuxToggle,
    sandbox,
    sandboxedCodebase,
    sandboxedCodebaseUnavailableReason,
    selectedAgentValue,
    selectedBaseBranch,
    selectedBranchToWorkOn,
    semanticIndex,
    sessionAgent,
    sessionAttachments,
    sessionLaunch,
    sessionsError,
    setAgent,
    setBranchConflict,
    setBranchIntent,
    setCodebaseDaemonInstanceId,
    setCreateRemoteBranch,
    setDaemonInstanceId,
    setDangerouslySkipPermissions,
    setDisplayName,
    setInitialPrompt,
    setManagedCodebase,
    setNewBranchName,
    setPermissionMode,
    setPrStackBaseSessionId,
    setProjectId,
    setProviderChoice,
    setRecipe,
    setSandbox,
    setSelectedBaseBranch,
    setSelectedBranchToWorkOn,
    setSemanticIndex,
    setSessionType,
    setSshConfigHost,
    setStackParent,
    sshConfigHost,
    sshListDaemonId,
    stackBaseSessionOptions,
    stackParent,
    stackParentOptions,
    startProgress,
    startingHostId,
    startingHostLabel,
    submitting,
    sessionType,
    baseBranchOptions,
    toolsError,
    typeOptions,
  };
}

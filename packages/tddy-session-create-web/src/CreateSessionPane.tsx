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
import { CreateSessionNameField } from "./CreateSessionNameField";
import { CreateSessionAttachmentsSection } from "./CreateSessionAttachmentsSection";
import { CreateSessionActions } from "./CreateSessionActions";
import { CreateSessionStartProgress } from "./CreateSessionStartProgress";
import { useProjectBranches } from "./useProjectBranches";
import { useCreateSessionCatalogs } from "./useCreateSessionCatalogs";
import { useCreateSessionPlacement } from "./useCreateSessionPlacement";
import { useCreateSessionModel } from "./useCreateSessionModel";
import { useCreateSessionEffort } from "./useCreateSessionEffort";
import { useCreateSessionSubmit } from "./useCreateSessionSubmit";
import type { CreateSessionPaneProps } from "./createSessionPaneProps";
import { useCreateSessionAgentSelection } from "./useCreateSessionAgentSelection";
import { BranchConflictDialog } from "./BranchConflictDialog";
import { StartPassphrasePrompt } from "./StartPassphrasePrompt";
import { useCreateSessionOrchestrator } from "./useCreateSessionOrchestrator";
import { useStartingHost } from "./useStartingHost";
import { useCreateSessionPaneState } from "./useCreateSessionPaneState";
import { CreateSessionLaunchFields } from "./CreateSessionLaunchFields";
import { useOpenOnMainSessionType } from "./useOpenOnMainSessionType";
import { useDaemons, useSelectedDaemon } from "tddy-client-web";
import { InlineError } from "tddy-ui-web";

type BranchIntent = BranchWorktreeIntent;

export type {
  CreateSessionInitialValues,
  CreateSessionPaneProps,
} from "./createSessionPaneProps";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function CreateSessionPane(props: CreateSessionPaneProps) {
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
  const {
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
    sessionDaemonInstanceId,
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
    workQueueToggle,
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
    setProjectId,
    setProviderChoice,
    setRecipe,
    setSandbox,
    setSelectedBaseBranch,
    setSelectedBranchToWorkOn,
    setSemanticIndex,
    setSessionType,
    setSshConfigHost,
    sshConfigHost,
    sshListDaemonId,
    startProgress,
    startingHostId,
    startingHostLabel,
    submitting,
    toolsError,
    typeOptions,
    sessionType,
    baseBranchOptions,
  } = useCreateSessionPaneState(props);
  return (
    <div
      data-testid="create-session-pane"
      className="flex flex-col h-full overflow-y-auto p-4 space-y-4"
    >
      {!projectHome && <h2 className="text-sm font-semibold">New session</h2>}

      {/* Session type toggle; the main variant disables every type that cannot run main. */}
      <CreateSessionTypeToggle
        options={typeOptions}
        sessionType={sessionType}
        setSessionType={setSessionType}
        disabledReason={
          projectHome
            ? (type) =>
                whyMainCannotRun(
                  sessionLaunchOf(type, typeOptions),
                  agentForSessionType(
                    offeredAgents,
                    sessionDaemonInstanceId,
                    connectedInstanceId,
                    type,
                  ),
                )
            : undefined
        }
      />

      {!projectHome && (
        <CreateSessionHostAndProjectFields
          daemons={daemons}
          daemonInstanceId={daemonInstanceId}
          setDaemonInstanceId={setDaemonInstanceId}
          projects={projects}
          projectOptions={projectOptions}
          projectsError={projectsError}
          projectId={projectId}
          setProjectId={setProjectId}
        />
      )}

      {/* A tool session's tddy-coder writes its own record, and main is named by its project:
          the daemon names neither, so neither offers a name. */}
      {!projectHome && sessionLaunch !== TDDY_CODER_LAUNCH && (
        <CreateSessionNameField displayName={displayName} setDisplayName={setDisplayName} />
      )}

      {accountsClient && providerHarness !== null && (
        <CreateSessionProviderField
          harnessRoutable={providerRoutes.harnessRoutable}
          choices={providerRoutes.choices}
          value={providerChoice ?? BUILT_IN_PROVIDER}
          onChange={setProviderChoice}
          error={providerRoutes.error}
        />
      )}

      <CreateSessionLaunchFields
        sessionLaunch={sessionLaunch}
        tool={{
          offeredAgents: offeredAgents,
          offeredHostFailures: offeredHostFailures,
          hostsAdvertised: hostsAdvertised,
          selectedAgentValue: selectedAgentValue,
          setAgent: setAgent,
          setDaemonInstanceId: setDaemonInstanceId,
          recipe: recipe,
          setRecipe: setRecipe,
          modelField: modelField,
          toolsError: toolsError,
        }}
        cli={sessionAgent === undefined ? null : {
          showsPermission: bindsParam(sessionAgent, "permission_mode"),
          permissionModes: sessionAgent.permissionModes,
          showsManaged: hasCapability(sessionAgent, "managed_workflow"),
          showsSandbox: allowsPlacement(sessionAgent, "jail"),
          showsSandboxedCodebase: hasCapability(sessionAgent, "split_host"),
          modelField: bindsParam(sessionAgent, "model") ? modelField : null,
          effortField: effortField,
          agentPickerSection: agentPickerSection,
          permissionMode: permissionMode,
          setPermissionMode: setPermissionMode,
          dangerouslySkipPermissions: dangerouslySkipPermissions,
          setDangerouslySkipPermissions: setDangerouslySkipPermissions,
          placementWithdrawsPermissionBypass: placementWithdrawsPermissionBypass,
          sandbox: sandbox,
          setSandbox: setSandbox,
          runInTmuxToggle: runInTmuxToggle,
          workQueueToggle: workQueueToggle,
          applyPlacement: applyPlacement,
          sandboxedCodebase: sandboxedCodebase,
          sandboxedCodebaseUnavailableReason: sandboxedCodebaseUnavailableReason,
          jailSharesTheFilesystemRoot: jailSharesTheFilesystemRoot,
          initialPrompt: initialPrompt,
          setInitialPrompt: setInitialPrompt,
          managedCodebase: managedCodebase,
          setManagedCodebase: setManagedCodebase,
          semanticIndex: semanticIndex,
          setSemanticIndex: setSemanticIndex,
          setSelectedAgentIds: agentPicks.setSelectedAgentIds,
          isSplitCodebase: isSplitCodebase,
          recipe: recipe,
          setRecipe: setRecipe,
          canChooseCodebaseHost: canChooseCodebaseHost,
          codebaseDaemonInstanceId: codebaseDaemonInstanceId,
          setCodebaseDaemonInstanceId: setCodebaseDaemonInstanceId,
          daemons: daemons,
          showsSshHost: hasCapability(sessionAgent, "split_host"),
          sshListDaemonId: sshListDaemonId,
          sessionToken: sessionToken,
          sshConfigHost: sshConfigHost,
          setSshConfigHost: setSshConfigHost,
          placementFixed: projectHome !== undefined || asOrchestrator,
        }}
      />

      {orchestratorField}

      {/* main's and an orchestrator's worktree, branch and stack are their project's. */}
      {!projectHome && !asOrchestrator && (
        <>
          <InlineError
            testId="create-session-sessions-load-error"
            message={sessionsError && `Sessions unavailable: ${sessionsError}`}
          />

          <CreateSessionBranchFields
            branchIntent={branchIntent}
            setBranchIntent={setBranchIntent}
            baseBranchLabel={initialValues?.baseBranchLabel}
            initialStackParent={initialValues?.workQueueParent}
            baseBranchOptions={baseBranchOptions}
            selectedBaseBranch={selectedBaseBranch}
            setSelectedBaseBranch={setSelectedBaseBranch}
            newBranchName={newBranchName}
            setNewBranchName={setNewBranchName}
            sessionLaunch={sessionLaunch}
            createRemoteBranch={createRemoteBranch}
            setCreateRemoteBranch={setCreateRemoteBranch}
            remoteBranches={remoteBranches}
            remoteBranchesError={remoteBranchesError}
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
        </>
      )}

      {/* The steps of the start, under the host doing the work (the codebase host on a split start). */}
      <CreateSessionStartProgress hostLabel={startingHostLabel} progress={startProgress} />

      {/* A start that must clone can block on the host's ssh key passphrase, and the host doing the
          cloning is the one that asks: the codebase host on a split session (the workspace start is
          forwarded there), the session's own host otherwise. */}
      <StartPassphrasePrompt
        instanceId={startingHostId}
        active={submitting}
        sessionToken={sessionToken}
      />

      <CreateSessionActions
        error={error}
        submitting={submitting}
        isSubmitEnabled={isSubmitEnabled}
        onCancel={onCancel}
        startCancel={projectHome ? undefined : created.startCancel}
        handleSubmit={handleSubmit}
        submitLabel={projectHome ? "Start main session" : undefined}
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

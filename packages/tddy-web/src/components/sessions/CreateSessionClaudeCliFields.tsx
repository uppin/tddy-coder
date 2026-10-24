import type { ReactNode } from "react";
import type { DaemonHost } from "../../lib/participantRole";
import { inputClass, labelClass } from "./createSessionFormStyles";
import type { SessionType } from "./createSessionRequest";
import type { CodebasePlacementChoice } from "./codebasePlacement";
import { CreateSessionPermissionFields } from "./CreateSessionPermissionFields";
import { CreateSessionSandboxedCodebaseToggle } from "./CreateSessionSandboxedCodebaseToggle";
import { CreateSessionManagedCodebaseFields } from "./CreateSessionManagedCodebaseFields";

export interface CreateSessionClaudeCliFieldsProps {
  modelField: ReactNode;
  agentPickerSection: ReactNode;
  permissionMode: string;
  setPermissionMode: (permissionMode: string) => void;
  dangerouslySkipPermissions: boolean;
  setDangerouslySkipPermissions: (dangerouslySkipPermissions: boolean) => void;
  placementWithdrawsPermissionBypass: boolean;
  sandbox: boolean;
  setSandbox: (sandbox: boolean) => void;
  applyPlacement: (toggled: CodebasePlacementChoice, on: boolean) => void;
  sandboxedCodebase: boolean;
  sandboxedCodebaseUnavailableReason: string | null;
  jailSharesTheFilesystemRoot: boolean;
  initialPrompt: string;
  setInitialPrompt: (initialPrompt: string) => void;
  managedCodebase: boolean;
  setManagedCodebase: (managedCodebase: boolean) => void;
  semanticIndex: boolean;
  setSemanticIndex: (semanticIndex: boolean) => void;
  setSelectedAgentIds: (agentIds: string[]) => void;
  isSplitCodebase: boolean;
  recipe: string;
  setRecipe: (recipe: string) => void;
  canChooseCodebaseHost: boolean;
  codebaseDaemonInstanceId: string;
  setCodebaseDaemonInstanceId: (codebaseDaemonInstanceId: string) => void;
  daemons: DaemonHost[];
  sessionType: SessionType;
  sshListDaemonId: string;
  sessionToken: string;
  sshConfigHost: string;
  setSshConfigHost: (sshConfigHost: string) => void;
  /**
   * The placement is not the operator's to choose: main runs managed on its project's home
   * worktree, so the Sandbox, Sandboxed codebase and Managed codebase controls (and the codebase
   * host under the last) are not offered.
   */
  placementFixed?: boolean;
}

/** Claude CLI session fields. */
export function CreateSessionClaudeCliFields({
  modelField,
  agentPickerSection,
  permissionMode,
  setPermissionMode,
  dangerouslySkipPermissions,
  setDangerouslySkipPermissions,
  placementWithdrawsPermissionBypass,
  sandbox,
  setSandbox,
  applyPlacement,
  sandboxedCodebase,
  sandboxedCodebaseUnavailableReason,
  jailSharesTheFilesystemRoot,
  initialPrompt,
  setInitialPrompt,
  managedCodebase,
  setManagedCodebase,
  semanticIndex,
  setSemanticIndex,
  setSelectedAgentIds,
  isSplitCodebase,
  recipe,
  setRecipe,
  canChooseCodebaseHost,
  codebaseDaemonInstanceId,
  setCodebaseDaemonInstanceId,
  daemons,
  sessionType,
  sshListDaemonId,
  sessionToken,
  sshConfigHost,
  setSshConfigHost,
  placementFixed = false,
}: CreateSessionClaudeCliFieldsProps) {
  /**
   * The specialized-agent picker and the Semantic index, offered on **every** claude-cli placement.
   *
   * Neither is a property of a placement. An agent is placeable on any host and reads the codebase
   * through the session's own placement; the index is built wherever the worktree is. They used to
   * live inside the Managed-codebase block, so choosing the jailed codebase — the placement where
   * delegating is safest, because the checkout an agent could damage is the confined one — took
   * both controls off the page. See
   * docs/ft/daemon/amendments/PRD-2026-09-20-sandboxed-codebase-managed-workflow.md.
   */
  const placementIndependentFields = (
    <>
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
    </>
  );

  const toggleManagedCodebase = (on: boolean) => {
    setManagedCodebase(on);
    // Switching the orchestration off resets what a managed session was going to be given, rather
    // than leaving the values to be stripped at submit: a request that disagrees with the screen is
    // how a picked agent went missing without an error. Both controls stay visible either way —
    // they are placement-independent — so this is a reset the operator can see, not a hidden value
    // dropped at the wire.
    if (!on) {
      setSemanticIndex(false);
      setSelectedAgentIds([]);
    }
    applyPlacement("managed", on);
  };

  return (
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
        placementFixed={placementFixed}
      />

      {!placementFixed && (
        <CreateSessionSandboxedCodebaseToggle
          sandboxedCodebase={sandboxedCodebase}
          sandboxedCodebaseUnavailableReason={sandboxedCodebaseUnavailableReason}
          jailSharesTheFilesystemRoot={jailSharesTheFilesystemRoot}
          applyPlacement={applyPlacement}
        />
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

      {/* Managed codebase — an explicit toggle that, when on, makes the session workflow-aware:
          a recipe, a seeded changeset and a toolcall listener. It is the orchestration axis and
          confines nothing, so it composes with any placement — including the jailed codebase.
          See docs/ft/coder/managed-codebase-workflow.md. */}
      {!placementFixed && (
        <div>
          <label className="flex items-center gap-2 text-sm text-muted-foreground">
            <input
              data-testid="create-session-managed-codebase-toggle"
              type="checkbox"
              className="h-4 w-4 rounded border-input"
              checked={managedCodebase}
              onChange={(e) => toggleManagedCodebase(e.target.checked)}
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
            />
          )}
        </div>
      )}

      {placementIndependentFields}
    </>
  );
}

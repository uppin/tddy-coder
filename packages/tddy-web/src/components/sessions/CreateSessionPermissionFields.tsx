import type { CodebasePlacementChoice } from "./codebasePlacement";
import { inputClass, labelClass } from "./createSessionFormStyles";

export interface CreateSessionPermissionFieldsProps {
  permissionMode: string;
  setPermissionMode: (permissionMode: string) => void;
  dangerouslySkipPermissions: boolean;
  setDangerouslySkipPermissions: (dangerouslySkipPermissions: boolean) => void;
  /** Whether the chosen placement withholds `--dangerously-skip-permissions` altogether. */
  placementWithdrawsPermissionBypass: boolean;
  sandbox: boolean;
  setSandbox: (sandbox: boolean) => void;
  applyPlacement: (toggled: CodebasePlacementChoice, on: boolean) => void;
}

/**
 * The claude-cli permission controls: permission mode, the permission bypass (withdrawn entirely
 * on a placement that rests on the agent's deny list) and the agent **Sandbox** placement.
 * Presentational — every value stays in `CreateSessionPane`.
 */
export function CreateSessionPermissionFields({
  permissionMode,
  setPermissionMode,
  dangerouslySkipPermissions,
  setDangerouslySkipPermissions,
  placementWithdrawsPermissionBypass,
  sandbox,
  setSandbox,
  applyPlacement,
}: CreateSessionPermissionFieldsProps) {
  return (
    <>
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

      {/* A split session and a jailed-codebase session both run their agent unjailed on this
          host, and both rest their entire "no route to the local filesystem" guarantee on the
          agent's deny list. Whether that list survives --dangerously-skip-permissions is not
          something this repo pins, so the combination is withdrawn rather than assumed safe —
          and the daemon refuses it by name on either placement. */}
      {!placementWithdrawsPermissionBypass && (
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

      {/* On a co-located placement the sandbox confines the agent on this daemon. On a split
          placement it confines the codebase host — the jail runs on the daemon holding the
          checkout, not the agent host. The combination with codebase_daemon_instance_id is
          admitted and the jail is placed on the codebase host. */}
      <div>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input
            data-testid="create-session-sandbox-toggle"
            type="checkbox"
            className="h-4 w-4 rounded border-input"
            checked={sandbox}
            onChange={(e) => {
              setSandbox(e.target.checked);
              applyPlacement("sandbox", e.target.checked);
            }}
          />
          Sandbox
        </label>
      </div>
    </>
  );
}

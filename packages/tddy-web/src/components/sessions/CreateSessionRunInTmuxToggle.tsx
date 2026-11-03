import type { SessionType } from "./createSessionRequest";

export interface CreateSessionRunInTmuxToggleProps {
  runInTmux: boolean;
  setRunInTmux: (runInTmux: boolean) => void;
  /**
   * The placement rules out a tmux-run agent (sandbox, jailed codebase or split placement), so the
   * checkbox is disabled. `useCreateSessionPlacement` owns that rule and clears the choice with it.
   */
  unavailable: boolean;
}

/**
 * "Run the agent in tmux", offered for claude-cli and every CLI harness type (`#tmux-agent` 3/3,
 * docs/ft/daemon/cursor-cli-session.md § tmux mode and its siblings). One control for all of them,
 * so the label, the enable rule and the test id cannot drift between session types.
 */
export function CreateSessionRunInTmuxToggle({
  runInTmux,
  setRunInTmux,
  unavailable,
}: CreateSessionRunInTmuxToggleProps) {
  return (
    <div>
      <label className="flex items-center gap-2 text-sm text-muted-foreground">
        <input
          data-testid="create-session-tmux-toggle"
          type="checkbox"
          className="h-4 w-4 rounded border-input"
          checked={runInTmux}
          onChange={(e) => setRunInTmux(e.target.checked)}
          disabled={unavailable}
        />
        Run the agent in tmux
      </label>
    </div>
  );
}

/**
 * Whether the pane shows the toggle in its shared slot for `sessionType`: every CLI harness type
 * but claude-cli, which places it among its own placement fields.
 */
export function showsTheSharedTmuxToggle(sessionType: SessionType): boolean {
  return (
    sessionType === "cursor-cli" ||
    sessionType === "opencode-cli" ||
    sessionType === "muse-cli" ||
    sessionType === "warp-cli"
  );
}

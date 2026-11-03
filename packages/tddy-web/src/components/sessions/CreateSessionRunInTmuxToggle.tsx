export interface CreateSessionRunInTmuxToggleProps {
  runInTmux: boolean;
  setRunInTmux: (runInTmux: boolean) => void;
  /** Tmux and the sandbox are exclusive: the option is disabled while the session is sandboxed. */
  sandbox: boolean;
}

/**
 * "Run the agent in tmux", offered for every CLI harness type (`#tmux-agent` 3/3,
 * docs/ft/daemon/1-WIP/PRD-2026-11-03-harnesses-in-tmux.md).
 */
export function CreateSessionRunInTmuxToggle(_props: CreateSessionRunInTmuxToggleProps) {
  // TODO(harnesses-in-tmux): implement — a labelled checkbox, test id `create-session-tmux-toggle`.
  return null;
}

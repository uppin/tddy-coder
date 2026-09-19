import type { CodebasePlacementChoice } from "./codebasePlacement";

export interface CreateSessionSandboxedCodebaseToggleProps {
  sandboxedCodebase: boolean;
  /** Why the session's host cannot jail its checkout, or `null` when it can. */
  sandboxedCodebaseUnavailableReason: string | null;
  /** What that host's jail leaves unconfined, when it says so. */
  jailSharesTheFilesystemRoot: boolean;
  applyPlacement: (toggled: CodebasePlacementChoice, on: boolean) => void;
}

/**
 * The **Sandboxed codebase** placement control, with the reason a host cannot serve it and the
 * caveat a host that shares its filesystem root must state. Presentational: the placement algebra
 * is `codebasePlacement.ts`'s and the state is `CreateSessionPane`'s.
 */
export function CreateSessionSandboxedCodebaseToggle({
  sandboxedCodebase,
  sandboxedCodebaseUnavailableReason,
  jailSharesTheFilesystemRoot,
  applyPlacement,
}: CreateSessionSandboxedCodebaseToggleProps) {
  return (
    <>
      {/* Sandboxed codebase — the inverted placement: the checkout goes in the
          `--workspace-tools` jail and the agent runs beside it, unconfined, reaching the code
          only through mcp__tddy-tools__*. Offered here only: claude-cli is the one agent whose
          native filesystem and shell tools can be withdrawn, so it is the only session type the
          daemon accepts this placement for.
          See docs/ft/daemon/amendments/PRD-2026-09-18-sandboxed-codebase-from-the-web.md. */}
      <div>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input
            data-testid="create-session-sandboxed-codebase-toggle"
            type="checkbox"
            className="h-4 w-4 rounded border-input"
            checked={sandboxedCodebase}
            // Disabled, never hidden: a capability the operator cannot see is one they cannot
            // learn they are on the wrong host for.
            disabled={sandboxedCodebaseUnavailableReason !== null}
            onChange={(e) => applyPlacement("sandboxedCodebase", e.target.checked)}
          />
          Sandboxed codebase
        </label>
        {sandboxedCodebaseUnavailableReason !== null && (
          <p
            data-testid="create-session-sandboxed-codebase-unavailable"
            className="mt-1 pl-6 text-xs text-muted-foreground"
          >
            {sandboxedCodebaseUnavailableReason}
          </p>
        )}
        {/* What this host's jail does not confine, said beside an offer of it. Today's Linux
            cgroups jail shares the host filesystem root — the minimal read-only root with
            pivot_root is unbuilt (docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md) — so
            "sandboxed" must not be left to promise more than it delivers. */}
        {jailSharesTheFilesystemRoot && (
          <p
            data-testid="create-session-sandboxed-codebase-caveat"
            className="mt-1 pl-6 text-xs text-muted-foreground"
          >
            This host&rsquo;s jail confines the session&rsquo;s processes and network, but not
            filesystem writes outside the checkout.
          </p>
        )}
      </div>
    </>
  );
}

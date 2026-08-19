import { useEffect, useState } from "react";
import { ConnectionService, type ProjectEntry } from "../../gen/connection_pb";
import { useAuthContext } from "../../hooks/authProvider";
import { safeTestIdPart } from "../../lib/testId";
import { useDaemonClientFor } from "../../rpc/selectedDaemon";
import type { AssistantRow } from "../../utils/mergeRegistryEntries";
import { ModelsDialogShell } from "./ModelsDialogShell";
import { noConnectionTo } from "../../rpc/useHostFanOut";
import { errorTextOf } from "./useModelRegistryFanOut";

/**
 * Where a tool-bearing assistant's tools may run, chosen before its chat opens.
 *
 * An assistant's tools execute **in the daemon process**, so the daemon confines them by path: the
 * ACP `cwd` is canonicalised and has to resolve inside one of the caller's own roots — their
 * sessions base, or the `main_repo_path` / `host_repo_paths` of their own `projects.yaml` — and an
 * empty `cwd` is refused outright (`model_registry::workspace::resolve_chat_workspace`).
 *
 * So the choice is offered from the one list that is guaranteed to satisfy that rule: the projects
 * **that daemon's own registry** holds for this operator (`ConnectionService.ListProjects` with
 * `local_only`, read straight off the owning daemon). Both sides read the same `projects.yaml`
 * through `projects_path_for_user`, so every `main_repo_path` offered here is by construction one
 * of the roots the daemon will accept — rather than a free-text path the operator has to guess and
 * the daemon then refuses.
 *
 * `local_only` is what keeps the list honest: a fanned-out `ListProjects` also returns *peers'*
 * rows, whose paths exist on other hosts and would be refused by this one.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md § Known risks — `Shell` as an
 * assignable assistant tool.
 */

/** How the projects of the assistant's daemon are known — an empty list is a claim, not a gap. */
type WorkspaceOptions =
  | { readonly status: "loading" }
  | { readonly status: "unavailable"; readonly error: string }
  | { readonly status: "ready"; readonly projects: readonly ProjectEntry[] };

export function ChatWorkspaceDialog({
  assistant,
  onChoose,
  onClose,
}: {
  assistant: AssistantRow;
  /** Called with the chosen workspace path, on the daemon that owns the assistant. */
  onChoose: (cwd: string) => void;
  onClose: () => void;
}) {
  const client = useDaemonClientFor(ConnectionService, assistant.daemonInstanceId);
  const { sessionToken } = useAuthContext();
  const [options, setOptions] = useState<WorkspaceOptions>({ status: "loading" });

  useEffect(() => {
    if (!client) {
      setOptions({
        status: "unavailable",
        error: noConnectionTo(assistant.daemonInstanceId),
      });
      return;
    }
    let cancelled = false;
    client
      .listProjects({ sessionToken: sessionToken ?? "", localOnly: true })
      .then((response) => {
        if (!cancelled) setOptions({ status: "ready", projects: response.projects });
      })
      .catch((err: unknown) => {
        // A failed read must not become an empty picker: "this host has no project" is a different
        // sentence from "nobody could ask it", and only one of them is the operator's problem.
        if (!cancelled) setOptions({ status: "unavailable", error: errorTextOf(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, assistant.daemonInstanceId]);

  return (
    <ModelsDialogShell
      testId="models-chat-workspace-dialog"
      label={`Choose where ${assistant.label}'s tools run`}
      className="flex w-full max-w-lg flex-col gap-3 rounded-md border border-border bg-background p-4 text-sm text-foreground"
      onClose={onClose}
    >
      <div>
        <h2 className="text-sm font-semibold">Where should {assistant.label}&apos;s tools run?</h2>
        <p className="mt-1 text-xs text-muted-foreground">
          {assistant.label} runs {assistant.tools.join(" · ")} on {assistant.daemonInstanceId}.
          Choose one of your projects there — the daemon refuses anything outside them.
        </p>
      </div>

      <div data-testid="models-chat-workspace-options" className="flex flex-col gap-2">
        {options.status === "loading" ? (
          <div
            data-testid="models-chat-workspace-empty"
            data-workspace-status="loading"
            className="rounded-md border border-border p-3 text-sm text-muted-foreground"
          >
            Reading {assistant.daemonInstanceId}&apos;s projects…
          </div>
        ) : null}

        {options.status === "unavailable" ? (
          <div
            data-testid="models-chat-workspace-error"
            data-workspace-status="unavailable"
            className="rounded-md border border-border p-3 text-sm text-destructive"
          >
            {options.error}
          </div>
        ) : null}

        {options.status === "ready"
          ? options.projects.map((project) => (
              <button
                key={project.projectId}
                type="button"
                data-testid={`models-chat-workspace-${safeTestIdPart(project.projectId)}`}
                data-workspace-path={project.mainRepoPath}
                className="rounded-md border border-input p-3 text-left text-sm text-foreground hover:bg-accent"
                onClick={() => onChoose(project.mainRepoPath)}
              >
                <div className="font-medium">{project.name}</div>
                <div className="text-xs text-muted-foreground">{project.mainRepoPath}</div>
              </button>
            ))
          : null}

        {options.status === "ready" && options.projects.length === 0 ? (
          <div
            data-testid="models-chat-workspace-empty"
            data-workspace-status="ready"
            className="rounded-md border border-border p-3 text-sm text-muted-foreground"
          >
            {assistant.daemonInstanceId} holds no project of yours for these tools to run in. Add
            one on the Projects screen first.
          </div>
        ) : null}
      </div>

      <div className="flex justify-end">
        <button
          type="button"
          data-testid="models-chat-workspace-cancel"
          className="rounded-md border border-input px-3 py-1 text-sm font-medium text-foreground hover:bg-accent"
          onClick={onClose}
        >
          Cancel
        </button>
      </div>
    </ModelsDialogShell>
  );
}

import React, { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionEntry, SessionService } from "tddy-client-web/gen/session_pb";
import type { ProjectService } from "tddy-client-web/gen/project_pb";
import type { CatalogService } from "tddy-client-web/gen/catalog_pb";
import type { WorkQueueService, WorkQueueAddress as WireWorkQueueAddress } from "tddy-client-web/gen/work_queue_pb";
import type { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import type { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import type { TaskService } from "tddy-client-web/gen/tasks_pb";
import { Button } from "tddy-ui-web";
import { WorkItemList } from "./WorkItemList";
import { AddWorkItemForm, type AddWorkItemFormSubmission } from "./AddWorkItemForm";
import { parseWorkQueue, type WorkItem } from "./workQueue";
import { hydrateWorkItems } from "./hydrateWorkItems";
import { workItemSessions, type StackChildMetadata } from "./workItemSessions";
import { useQueryBranch } from "./useQueryBranch";
import { buildBranchQueries } from "./branchQueries";
import { baseSyncView, canPullFromBase } from "./baseSyncStatus";
import { DirtyWorktreeDialog, type DirtyWorktreePrompt } from "./DirtyWorktreeDialog";
import { resolveRepointTarget } from "./startBlockers";
import { CreateSessionDialog, remoteTrackingName, type CreateSessionInitialValues } from "tddy-session-create-web";
import { deriveStackBaseBranch } from "./deriveStackBaseBranch";
import { baseBranchChoice } from "./baseBranchChoice";
import { WorkItemLinksContext } from "./workItemLinks";
import { useWorkItemLinksState } from "./useWorkItemLinksState";

/** The fields of a `work_queue.WorkQueueAddress` the view is opened on. */
export type WorkQueueAddress = Pick<WireWorkQueueAddress, "projectId" | "queueId" | "daemonInstanceId">;

export interface WorkQueueViewProps {
  /**
   * The queue to show — its owning session's (`queue_id` is that session's id, `#workqueue` 5/14):
   * every request the view makes carries this address, and the daemon it names serves them —
   * forwarded there by whichever daemon the view's clients reach. A queue is created only when its
   * session starts, so the view never makes one.
   */
  address: WorkQueueAddress;
  /** `work_queue.WorkQueueService` — `GetWorkQueue`, `QueryBranch` and every planned-PR control. */
  workQueueClient?: Client<typeof WorkQueueService>;
  sessionToken?: string;
  /** The full session list (all hosts) — each planned node's child session is resolved from it. */
  sessions?: SessionEntry[];
  /** The `session` metadata block each live participant publishes, keyed by session id (D37, D38). */
  sessionMetadataBySessionId?: ReadonlyMap<string, StackChildMetadata>;
  /** The project's default branch (`ProjectEntry.main_branch_ref`); empty for a legacy project (D20). */
  defaultBranch?: string;
  /** The project's default remote (`ProjectEntry.default_remote`); the view falls back to `origin`. */
  defaultRemote?: string;
  /** Select and attach a session — how a spawned row's status chip opens its child. */
  onOpenSession?: (sessionId: string) => void;
  /** Fired after a child session is spawned from a row, so the caller can show it in the drawer. */
  onChildSessionStarted?: (entry: { sessionId: string; recipe: string; projectId: string }) => void;
  /** The Start-session dialog's clients, on the host the stack lives on. The dialog is offered only with all five. */
  client?: Client<typeof SessionService>;
  projectClient?: Client<typeof ProjectService>;
  catalogClient?: Client<typeof CatalogService>;
  sessionFilesClient?: Client<typeof SessionFilesService>;
  worktreeClient?: Client<typeof WorktreeService>;
  taskClient?: Client<typeof TaskService>;
}

const NO_SESSION_METADATA: ReadonlyMap<string, StackChildMetadata> = new Map();

function withoutNode(errors: Record<string, string>, itemId: string): Record<string, string> {
  if (!(itemId in errors)) return errors;
  const next = { ...errors };
  delete next[itemId];
  return next;
}

function withoutNodeInFlight(inFlight: ReadonlySet<string>, itemId: string): ReadonlySet<string> {
  const next = new Set(inFlight);
  next.delete(itemId);
  return next;
}

function failureReason(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function unpushedPullReason(baseBranch: string, branch: string, pushError: string): string {
  return `Pulled ${baseBranch} into ${branch} locally, but the push failed — ${branch} on the remote (and its PR) does not have it yet: ${pushError}`;
}

/**
 * The **stack view** (`#orchestrators` 9/11): one of a project's PR stacks, opened by its id from the
 * drawer's Stacks row at `/sessions/project/:projectId/queue/:queueId`. Today's Planned PRs panel at
 * full width — the planned-PR list with every control it has (start session, repoint, reorder, add
 * planned PR, pull base) — loaded with `GetWorkQueue` and acting on the stack by its address, so it
 * needs no orchestrator session at all.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md
 */
export function WorkQueueView(props: WorkQueueViewProps): React.ReactElement {
  const {
    address,
    workQueueClient,
    sessionToken = "",
    sessions = [],
    sessionMetadataBySessionId = NO_SESSION_METADATA,
    defaultBranch = "",
    defaultRemote = "",
    onOpenSession,
    onChildSessionStarted,
    client,
    projectClient,
    catalogClient,
    sessionFilesClient,
    worktreeClient,
    taskClient,
  } = props;


  const [workQueueJson, setWorkQueueJson] = useState("");
  const [createdBySessionId, setCreatedBySessionId] = useState("");
  const [isAddingPlannedPr, setIsAddingPlannedPr] = useState(false);
  const [startSessionNode, setStartSessionNode] = useState<WorkItem | null>(null);
  const [repointErrorByNodeId, setRepointErrorByNodeId] = useState<Record<string, string>>({});
  const [branchMutatingNodeIds, setBranchMutatingNodeIds] = useState<ReadonlySet<string>>(new Set());
  const [reorderErrorByNodeId, setReorderErrorByNodeId] = useState<Record<string, string>>({});
  const [reorderingNodeIds, setReorderingNodeIds] = useState<ReadonlySet<string>>(new Set());
  const [syncErrorByNodeId, setSyncErrorByNodeId] = useState<Record<string, string>>({});
  const [dirtyWorktreePrompt, setDirtyWorktreePrompt] = useState<DirtyWorktreePrompt | null>(null);

  // Load the stack by its address; a mutation's response replaces the plan it answers with.
  useEffect(() => {
    if (!workQueueClient) return;
    let cancelled = false;
    workQueueClient
      .getWorkQueue({ sessionToken, stack: address })
      .then((res) => {
        if (cancelled) return;
        setWorkQueueJson(res.workQueueJson);
        setCreatedBySessionId(res.summary?.createdBySessionId ?? "");
      })
      .catch(() => {
        // Keep what is shown; a failed read must not blank a plan already loaded.
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workQueueClient, sessionToken, address.projectId, address.queueId, address.daemonInstanceId]);

  const stack = useMemo(() => parseWorkQueue(workQueueJson), [workQueueJson]);
  const childSessions = useMemo(
    () => workItemSessions(sessions, sessionMetadataBySessionId),
    [sessions, sessionMetadataBySessionId],
  );
  const nodes = useMemo(
    () => hydrateWorkItems(stack.nodes, childSessions, createdBySessionId),
    [stack.nodes, childSessions, createdBySessionId],
  );
  // The label the operator chose to filter by (`#workqueue` 7/14); empty shows every item.
  const [labelFilter, setLabelFilter] = useState("");
  const everyLabel = useMemo(
    () => [...new Set(nodes.flatMap((node) => node.labels))].sort(),
    [nodes],
  );
  const shownNodes = useMemo(
    () => (labelFilter ? nodes.filter((node) => node.labels.includes(labelFilter)) : nodes),
    [nodes, labelFilter],
  );
  const branchQueries = useMemo(
    () => buildBranchQueries(nodes, defaultBranch),
    [nodes, defaultBranch],
  );
  const { resolutionByBranch: branchResolutionByBranch, setResolution } = useQueryBranch(
    workQueueClient,
    sessionToken,
    { stack: address },
    branchQueries,
  );

  const handleAddPlannedPr = async (input: AddWorkItemFormSubmission) => {
    if (!workQueueClient) return;
    const res = await workQueueClient.addWorkItem({
      sessionToken,
      stack: address,
      title: input.title,
      description: input.description,
      branchSuggestion: input.branchSuggestion,
      parents: input.parents,
      childRecipe: "",
    });
    setWorkQueueJson(res.workQueueJson);
    if (input.startSession) {
      const added = parseWorkQueue(res.workQueueJson).nodes.find((node) => node.itemId === res.itemId);
      if (!added) {
        throw new Error(
          "The added planned PR is missing from the returned stack plan — no session was started.",
        );
      }
      setStartSessionNode(added);
    }
    setIsAddingPlannedPr(false);
  };

  const handleRepoint = async (itemId: string) => {
    if (!workQueueClient) return;
    const node = nodes.find((n) => n.itemId === itemId);
    if (!node) return;
    if (branchMutatingNodeIds.has(itemId)) return;
    setBranchMutatingNodeIds((prev) => new Set(prev).add(itemId));
    setRepointErrorByNodeId((prev) => withoutNode(prev, itemId));
    try {
      const res = await workQueueClient.repointWorkItem({
        sessionToken,
        stack: address,
        itemId,
        targetBaseBranch: resolveRepointTarget(node, nodes, branchResolutionByBranch, defaultBranch),
      });
      setWorkQueueJson(res.workQueueJson);
    } catch (err) {
      setRepointErrorByNodeId((errors) => ({ ...errors, [itemId]: failureReason(err) }));
    } finally {
      setBranchMutatingNodeIds((prev) => withoutNodeInFlight(prev, itemId));
    }
  };

  // `#workqueue` 6/14: a row's PR and session links, and its live check counts.
  const workItemLinks = useWorkItemLinksState({
    workQueueClient,
    sessionToken,
    address,
    sessions,
    onWorkQueueJson: setWorkQueueJson,
  });

  const handleReorder = async (itemId: string, direction: "up" | "down") => {
    if (!workQueueClient) return;
    if (reorderingNodeIds.has(itemId)) return;
    setReorderingNodeIds((prev) => new Set(prev).add(itemId));
    setReorderErrorByNodeId((prev) => withoutNode(prev, itemId));
    try {
      const res = await workQueueClient.reorderWorkItem({
        sessionToken,
        stack: address,
        itemId,
        direction,
      });
      setWorkQueueJson(res.workQueueJson);
    } catch (err) {
      setReorderErrorByNodeId((errors) => ({ ...errors, [itemId]: failureReason(err) }));
    } finally {
      setReorderingNodeIds((prev) => withoutNodeInFlight(prev, itemId));
    }
  };

  const runPull = async (pull: {
    itemId: string;
    branch: string;
    baseBranch: string;
    strategy: "merge" | "rebase";
    dirtyWorktreeAction: "" | "commit";
    commitMessage: string;
  }) => {
    if (!workQueueClient) return;
    const { itemId, branch } = pull;
    if (branchMutatingNodeIds.has(itemId)) return;
    setBranchMutatingNodeIds((prev) => new Set(prev).add(itemId));
    setSyncErrorByNodeId((prev) => withoutNode(prev, itemId));
    try {
      const res = await workQueueClient.pullBaseIntoBranch({
        sessionToken,
        stack: address,
        itemId,
        baseBranch: pull.baseBranch,
        strategy: pull.strategy,
        dirtyWorktreeAction: pull.dirtyWorktreeAction,
        commitMessage: pull.commitMessage,
      });
      if (res.resolution) setResolution(branch, res.resolution);
      if (!res.pushed && res.pushError) {
        setSyncErrorByNodeId((errors) => ({
          ...errors,
          [itemId]: unpushedPullReason(pull.baseBranch, branch, res.pushError),
        }));
      }
    } catch (err) {
      setSyncErrorByNodeId((errors) => ({ ...errors, [itemId]: failureReason(err) }));
    } finally {
      setBranchMutatingNodeIds((prev) => withoutNodeInFlight(prev, itemId));
    }
  };

  const handleSyncFromBase = (itemId: string, strategy: "merge" | "rebase") => {
    const node = nodes.find((n) => n.itemId === itemId);
    if (!node?.branch) return;
    if (branchMutatingNodeIds.has(itemId)) return;
    const resolution = branchResolutionByBranch[node.branch];
    const view = baseSyncView(resolution);
    if (!canPullFromBase(view)) return;
    if (resolution?.worktree?.dirty) {
      setDirtyWorktreePrompt({
        itemId,
        branch: node.branch,
        baseBranch: view.baseBranch,
        strategy,
        dirtyPaths: resolution.worktree.dirtyPaths,
      });
      return;
    }
    void runPull({
      itemId,
      branch: node.branch,
      baseBranch: view.baseBranch,
      strategy,
      dirtyWorktreeAction: "",
      commitMessage: "",
    });
  };

  const handleCommitDirtyWorktreeAndPull = (commitMessage: string) => {
    const prompt = dirtyWorktreePrompt;
    if (!prompt) return;
    setDirtyWorktreePrompt(null);
    void runPull({
      itemId: prompt.itemId,
      branch: prompt.branch,
      baseBranch: prompt.baseBranch,
      strategy: prompt.strategy,
      dirtyWorktreeAction: "commit",
      commitMessage,
    });
  };

  const handleStartSession = (node: WorkItem) => {
    if (!client) return;
    setStartSessionNode(node);
  };

  const remote = defaultRemote || "origin";
  const ownedBranch = startSessionNode?.branch ?? "";
  const choice = startSessionNode
    ? baseBranchChoice(startSessionNode, nodes, defaultBranch)
    : { options: [], selected: "" };
  const baseBranchOptions = choice.options.map((option) => {
    const ref = remoteTrackingName(option.value, remote);
    return { value: ref, label: option.label === option.value ? ref : option.label };
  });
  const startSessionInitialValues: CreateSessionInitialValues | undefined = startSessionNode
    ? {
        projectId: address.projectId,
        daemonInstanceId: address.daemonInstanceId,
        workQueueParent: createdBySessionId,
        workItemId: startSessionNode.itemId,
        branchIntent: ownedBranch ? "work_on_selected_branch" : "new_branch_from_base",
        selectedBranch: ownedBranch,
        newBranchName: ownedBranch ? "" : (startSessionNode.branchSuggestion ?? ""),
        baseBranchLabel: remoteTrackingName(
          deriveStackBaseBranch(startSessionNode, nodes, defaultBranch),
          remote,
        ),
        baseBranchOptions,
        selectedBaseBranch: remoteTrackingName(choice.selected, remote),
        createRemoteBranch: true,
        initialPrompt: [startSessionNode.title, startSessionNode.description]
          .filter(Boolean)
          .join("\n\n"),
      }
    : undefined;

  const handleChildSessionCreated = (sessionId: string) => {
    const node = startSessionNode;
    setStartSessionNode(null);
    if (!node) return;
    onChildSessionStarted?.({
      sessionId,
      recipe: node.childRecipe,
      projectId: address.projectId,
    });
  };

  return (
    <div
      data-testid="work-queue-view"
      data-stack-id={address.queueId}
      className="flex-1 min-h-0 flex flex-col overflow-hidden"
    >
      <div className="flex-shrink-0 flex justify-end gap-2 p-3 pb-0">
        <Button
          data-testid="work-item-add-btn"
          size="sm"
          variant="outline"
          onClick={() => setIsAddingPlannedPr(true)}
        >
          + New planned PR
        </Button>
      </div>
      {isAddingPlannedPr && (
        <AddWorkItemForm
          nodes={nodes}
          onSubmit={handleAddPlannedPr}
          onCancel={() => setIsAddingPlannedPr(false)}
        />
      )}
      {everyLabel.length > 0 && (
        <div className="flex-shrink-0 flex items-center gap-2 px-3 pt-2 text-xs">
          <label htmlFor="work-queue-label-filter">Label</label>
          <select
            id="work-queue-label-filter"
            data-testid="work-queue-label-filter"
            value={labelFilter}
            onChange={(event) => setLabelFilter(event.target.value)}
            className="rounded border border-border bg-background px-2 py-1"
          >
            <option value="">All</option>
            {everyLabel.map((label) => (
              <option key={label} value={label}>
                {label}
              </option>
            ))}
          </select>
        </div>
      )}
      <WorkItemLinksContext.Provider value={workItemLinks}>
      <WorkItemList
        nodes={shownNodes}
        onStartSession={handleStartSession}
        startingNodeId={startSessionNode?.itemId ?? null}
        sessions={sessions}
        branchResolutionByBranch={branchResolutionByBranch}
        onRepoint={handleRepoint}
        defaultBranch={defaultBranch}
        repointErrorByNodeId={repointErrorByNodeId}
        branchMutatingNodeIds={branchMutatingNodeIds}
        onReorder={handleReorder}
        reorderErrorByNodeId={reorderErrorByNodeId}
        reorderingNodeIds={reorderingNodeIds}
        onSyncFromBase={handleSyncFromBase}
        syncErrorByNodeId={syncErrorByNodeId}
        onOpenSession={onOpenSession}
        childSessions={childSessions}
        orchestratorSessionId={createdBySessionId}
        branchQueries={branchQueries}
      />
      </WorkItemLinksContext.Provider>
      <DirtyWorktreeDialog
        prompt={dirtyWorktreePrompt}
        onCommitAndPull={handleCommitDirtyWorktreeAndPull}
        onCancel={() => setDirtyWorktreePrompt(null)}
      />
      {client && projectClient && catalogClient && sessionFilesClient && worktreeClient && (
        <CreateSessionDialog
          open={startSessionNode !== null}
          client={client}
          projectClient={projectClient}
          catalogClient={catalogClient}
          sessionFilesClient={sessionFilesClient}
          worktreeClient={worktreeClient}
          taskClient={taskClient}
          sessionToken={sessionToken}
          initialValues={startSessionInitialValues}
          onClose={() => setStartSessionNode(null)}
          onCreated={handleChildSessionCreated}
        />
      )}
    </div>
  );
}

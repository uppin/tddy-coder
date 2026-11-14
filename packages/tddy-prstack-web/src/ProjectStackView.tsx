import React, { useEffect, useMemo, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { SessionEntry, SessionService } from "tddy-client-web/gen/session_pb";
import type { ProjectService } from "tddy-client-web/gen/project_pb";
import type { CatalogService } from "tddy-client-web/gen/catalog_pb";
import type { PrStackService, StackAddress } from "tddy-client-web/gen/pr_stack_pb";
import type { SessionFilesService } from "tddy-client-web/gen/session_files_pb";
import type { WorktreeService } from "tddy-client-web/gen/worktree_pb";
import type { TaskService } from "tddy-client-web/gen/tasks_pb";
import { Button } from "tddy-ui-web";
import { PlannedPrList } from "./PlannedPrList";
import { AddPlannedPrForm, type AddPlannedPrFormSubmission } from "./AddPlannedPrForm";
import { parseStackPlan, type StackNode } from "./stackPlan";
import { hydrateStackNodes } from "./hydrateStackNodes";
import { stackChildSessions, type StackChildMetadata } from "./stackChildSessions";
import { useQueryBranch } from "./useQueryBranch";
import { buildBranchQueries } from "./branchQueries";
import { baseSyncView, canPullFromBase } from "./baseSyncStatus";
import { DirtyWorktreeDialog, type DirtyWorktreePrompt } from "./DirtyWorktreeDialog";
import { resolveRepointTarget } from "./startBlockers";
import { CreateSessionDialog, remoteTrackingName, type CreateSessionInitialValues } from "tddy-session-create-web";
import { deriveStackBaseBranch } from "./deriveStackBaseBranch";
import { baseBranchChoice } from "./baseBranchChoice";

/** The fields of a `pr_stack.StackAddress` the view is opened on. */
export type ProjectStackAddress = Pick<StackAddress, "projectId" | "stackId" | "daemonInstanceId">;

export interface ProjectStackViewProps {
  /**
   * The stack to show, as the drawer's Stacks row listed it (`ListProjectStacks`): every request the
   * view makes carries this address, and the daemon it names serves them — forwarded there by
   * whichever daemon the view's clients reach.
   */
  address: ProjectStackAddress;
  /** `pr_stack.PrStackService` — `GetProjectStack`, `QueryBranch` and every planned-PR control. */
  prStackClient?: Client<typeof PrStackService>;
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

function withoutNode(errors: Record<string, string>, nodeId: string): Record<string, string> {
  if (!(nodeId in errors)) return errors;
  const next = { ...errors };
  delete next[nodeId];
  return next;
}

function withoutNodeInFlight(inFlight: ReadonlySet<string>, nodeId: string): ReadonlySet<string> {
  const next = new Set(inFlight);
  next.delete(nodeId);
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
 * drawer's Stacks row at `/sessions/project/:projectId/stack/:stackId`. Today's Planned PRs panel at
 * full width — the planned-PR list with every control it has (start session, repoint, reorder, add
 * planned PR, pull base) — loaded with `GetProjectStack` and acting on the stack by its address, so it
 * needs no orchestrator session at all.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md
 */
export function ProjectStackView(props: ProjectStackViewProps): React.ReactElement {
  const {
    address,
    prStackClient,
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

  const [stackPlanJson, setStackPlanJson] = useState("");
  const [createdBySessionId, setCreatedBySessionId] = useState("");
  const [isAddingPlannedPr, setIsAddingPlannedPr] = useState(false);
  const [startSessionNode, setStartSessionNode] = useState<StackNode | null>(null);
  const [repointErrorByNodeId, setRepointErrorByNodeId] = useState<Record<string, string>>({});
  const [branchMutatingNodeIds, setBranchMutatingNodeIds] = useState<ReadonlySet<string>>(new Set());
  const [reorderErrorByNodeId, setReorderErrorByNodeId] = useState<Record<string, string>>({});
  const [reorderingNodeIds, setReorderingNodeIds] = useState<ReadonlySet<string>>(new Set());
  const [syncErrorByNodeId, setSyncErrorByNodeId] = useState<Record<string, string>>({});
  const [dirtyWorktreePrompt, setDirtyWorktreePrompt] = useState<DirtyWorktreePrompt | null>(null);

  // Load the stack by its address; a mutation's response replaces the plan it answers with.
  useEffect(() => {
    if (!prStackClient) return;
    let cancelled = false;
    prStackClient
      .getProjectStack({ sessionToken, stack: address })
      .then((res) => {
        if (cancelled) return;
        setStackPlanJson(res.stackPlanJson);
        setCreatedBySessionId(res.summary?.createdBySessionId ?? "");
      })
      .catch(() => {
        // Keep what is shown; a failed read must not blank a plan already loaded.
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prStackClient, sessionToken, address.projectId, address.stackId, address.daemonInstanceId]);

  const stack = useMemo(() => parseStackPlan(stackPlanJson), [stackPlanJson]);
  const childSessions = useMemo(
    () => stackChildSessions(sessions, sessionMetadataBySessionId),
    [sessions, sessionMetadataBySessionId],
  );
  const nodes = useMemo(
    () => hydrateStackNodes(stack.nodes, childSessions, createdBySessionId),
    [stack.nodes, childSessions, createdBySessionId],
  );
  const branchQueries = useMemo(
    () => buildBranchQueries(nodes, defaultBranch),
    [nodes, defaultBranch],
  );
  const { resolutionByBranch: branchResolutionByBranch, setResolution } = useQueryBranch(
    prStackClient,
    sessionToken,
    { stack: address },
    branchQueries,
  );

  const handleAddPlannedPr = async (input: AddPlannedPrFormSubmission) => {
    if (!prStackClient) return;
    const res = await prStackClient.addPlannedPr({
      sessionToken,
      stack: address,
      title: input.title,
      description: input.description,
      branchSuggestion: input.branchSuggestion,
      parents: input.parents,
      childRecipe: "",
    });
    setStackPlanJson(res.stackPlanJson);
    if (input.startSession) {
      const added = parseStackPlan(res.stackPlanJson).nodes.find((node) => node.nodeId === res.nodeId);
      if (!added) {
        throw new Error(
          "The added planned PR is missing from the returned stack plan — no session was started.",
        );
      }
      setStartSessionNode(added);
    }
    setIsAddingPlannedPr(false);
  };

  const handleRepoint = async (nodeId: string) => {
    if (!prStackClient) return;
    const node = nodes.find((n) => n.nodeId === nodeId);
    if (!node) return;
    if (branchMutatingNodeIds.has(nodeId)) return;
    setBranchMutatingNodeIds((prev) => new Set(prev).add(nodeId));
    setRepointErrorByNodeId((prev) => withoutNode(prev, nodeId));
    try {
      const res = await prStackClient.repointPlannedPr({
        sessionToken,
        stack: address,
        nodeId,
        targetBaseBranch: resolveRepointTarget(node, nodes, branchResolutionByBranch, defaultBranch),
      });
      setStackPlanJson(res.stackPlanJson);
    } catch (err) {
      setRepointErrorByNodeId((errors) => ({ ...errors, [nodeId]: failureReason(err) }));
    } finally {
      setBranchMutatingNodeIds((prev) => withoutNodeInFlight(prev, nodeId));
    }
  };

  const handleReorder = async (nodeId: string, direction: "up" | "down") => {
    if (!prStackClient) return;
    if (reorderingNodeIds.has(nodeId)) return;
    setReorderingNodeIds((prev) => new Set(prev).add(nodeId));
    setReorderErrorByNodeId((prev) => withoutNode(prev, nodeId));
    try {
      const res = await prStackClient.reorderPlannedPr({
        sessionToken,
        stack: address,
        nodeId,
        direction,
      });
      setStackPlanJson(res.stackPlanJson);
    } catch (err) {
      setReorderErrorByNodeId((errors) => ({ ...errors, [nodeId]: failureReason(err) }));
    } finally {
      setReorderingNodeIds((prev) => withoutNodeInFlight(prev, nodeId));
    }
  };

  const runPull = async (pull: {
    nodeId: string;
    branch: string;
    baseBranch: string;
    strategy: "merge" | "rebase";
    dirtyWorktreeAction: "" | "commit";
    commitMessage: string;
  }) => {
    if (!prStackClient) return;
    const { nodeId, branch } = pull;
    if (branchMutatingNodeIds.has(nodeId)) return;
    setBranchMutatingNodeIds((prev) => new Set(prev).add(nodeId));
    setSyncErrorByNodeId((prev) => withoutNode(prev, nodeId));
    try {
      const res = await prStackClient.pullBaseIntoBranch({
        sessionToken,
        stack: address,
        nodeId,
        baseBranch: pull.baseBranch,
        strategy: pull.strategy,
        dirtyWorktreeAction: pull.dirtyWorktreeAction,
        commitMessage: pull.commitMessage,
      });
      if (res.resolution) setResolution(branch, res.resolution);
      if (!res.pushed && res.pushError) {
        setSyncErrorByNodeId((errors) => ({
          ...errors,
          [nodeId]: unpushedPullReason(pull.baseBranch, branch, res.pushError),
        }));
      }
    } catch (err) {
      setSyncErrorByNodeId((errors) => ({ ...errors, [nodeId]: failureReason(err) }));
    } finally {
      setBranchMutatingNodeIds((prev) => withoutNodeInFlight(prev, nodeId));
    }
  };

  const handleSyncFromBase = (nodeId: string, strategy: "merge" | "rebase") => {
    const node = nodes.find((n) => n.nodeId === nodeId);
    if (!node?.branch) return;
    if (branchMutatingNodeIds.has(nodeId)) return;
    const resolution = branchResolutionByBranch[node.branch];
    const view = baseSyncView(resolution);
    if (!canPullFromBase(view)) return;
    if (resolution?.worktree?.dirty) {
      setDirtyWorktreePrompt({
        nodeId,
        branch: node.branch,
        baseBranch: view.baseBranch,
        strategy,
        dirtyPaths: resolution.worktree.dirtyPaths,
      });
      return;
    }
    void runPull({
      nodeId,
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
      nodeId: prompt.nodeId,
      branch: prompt.branch,
      baseBranch: prompt.baseBranch,
      strategy: prompt.strategy,
      dirtyWorktreeAction: "commit",
      commitMessage,
    });
  };

  const handleStartSession = (node: StackNode) => {
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
        stackParent: createdBySessionId,
        stackNodeId: startSessionNode.nodeId,
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
      data-testid="project-stack-view"
      data-stack-id={address.stackId}
      className="flex-1 min-h-0 flex flex-col overflow-hidden"
    >
      <div className="flex-shrink-0 flex justify-end p-3 pb-0">
        <Button
          data-testid="pr-stack-add-planned-pr-btn"
          size="sm"
          variant="outline"
          onClick={() => setIsAddingPlannedPr(true)}
        >
          + New planned PR
        </Button>
      </div>
      {isAddingPlannedPr && (
        <AddPlannedPrForm
          nodes={nodes}
          onSubmit={handleAddPlannedPr}
          onCancel={() => setIsAddingPlannedPr(false)}
        />
      )}
      <PlannedPrList
        nodes={nodes}
        onStartSession={handleStartSession}
        startingNodeId={startSessionNode?.nodeId ?? null}
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

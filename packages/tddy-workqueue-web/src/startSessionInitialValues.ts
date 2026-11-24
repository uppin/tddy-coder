import { remoteTrackingName, type CreateSessionInitialValues } from "tddy-session-create-web";
import type { WorkItem } from "./workQueue";
import type { WorkQueueAddress } from "./WorkQueueView";
import { deriveStackBaseBranch } from "./deriveStackBaseBranch";
import { baseBranchChoice } from "./baseBranchChoice";

/**
 * What the Start-session dialog opens pre-filled with for `node` of the queue at `address`: the
 * node's own branch when it has one, else a new branch from the base its parents derive, offered as
 * `remote`'s tracking refs; the queue's owner as the session's work-queue parent; the node's title and
 * description as the first prompt.
 */
export function startSessionInitialValues(
  node: WorkItem,
  nodes: WorkItem[],
  {
    address,
    defaultBranch,
    remote,
    workQueueParent,
  }: { address: WorkQueueAddress; defaultBranch: string; remote: string; workQueueParent: string },
): CreateSessionInitialValues {
  const ownedBranch = node.branch ?? "";
  const choice = baseBranchChoice(node, nodes, defaultBranch);
  const baseBranchOptions = choice.options.map((option) => {
    const ref = remoteTrackingName(option.value, remote);
    return { value: ref, label: option.label === option.value ? ref : option.label };
  });
  return {
    projectId: address.projectId,
    daemonInstanceId: address.daemonInstanceId,
    workQueueParent,
    workItemId: node.itemId,
    branchIntent: ownedBranch ? "work_on_selected_branch" : "new_branch_from_base",
    selectedBranch: ownedBranch,
    newBranchName: ownedBranch ? "" : (node.branchSuggestion ?? ""),
    baseBranchLabel: remoteTrackingName(deriveStackBaseBranch(node, nodes, defaultBranch), remote),
    baseBranchOptions,
    selectedBaseBranch: remoteTrackingName(choice.selected, remote),
    createRemoteBranch: true,
    initialPrompt: [node.title, node.description].filter(Boolean).join("\n\n"),
  };
}

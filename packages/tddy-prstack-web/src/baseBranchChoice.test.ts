import { describe, expect, it } from "bun:test";
import { aStackNode } from "../../../test-utils";
import { baseBranchChoice } from "./baseBranchChoice";
import type { StackNode } from "./stackPlan";

/**
 * Tests for `baseBranchChoice` — the Start-Session dialog's "Base branch" `<select>` state: the
 * ordered option list *and* the pre-selection, resolved together so they cannot drift apart.
 *
 * They drifted before: the options came from `prioritiseBaseBranchOptions` (stack branches only) and
 * the pre-selection was `options[0]`, while the "New branch from base: <x>" label came from
 * `deriveStackBaseBranch`. A planned PR repointed onto the project default branch therefore showed
 * the label `origin/master` but pre-selected — and submitted — an unrelated stack branch, silently
 * undoing the repoint.
 *
 * The contract:
 *   1. The pre-selection is the node's derived stack base (`deriveStackBaseBranch`), never the first
 *      option that happens to be listed.
 *   2. The project default branch is always offered, so a node based onto it can be *shown* as such.
 *   3. The pre-selection is always one of the offered options — nothing else can be submitted than
 *      what the operator sees.
 *   4. When the stack offers no branch of its own there is nothing to choose: no options, empty
 *      selection, and the daemon resolves the default base itself.
 *
 * The `defaultBranch` passed here is the project's `main_branch_ref` as stored — a remote-tracking
 * ref (`origin/master`) — while a stack node's `branch` is a local name. `PrStackScreen` lifts both
 * into remote-tracking form (idempotently) for display; this layer keeps them as given.
 */

const DEFAULT_BRANCH = "origin/master";
const ATTACH_PROTO_BRANCH = "feature/session-attach-docs/attach-proto";
const ATTACH_STORE_BRANCH = "feature/session-attach-docs/attach-store";

/**
 * The live stack from session 019f9dd5 after "Create Session attach UI" was repointed onto master:
 * `attach-ui` lost its `attach-start` parent edge, while `attach-proto` and `attach-store` are open
 * PRs that own branches and `attach-start` is still planned (branchless).
 */
function anAttachDocsStackWithRepointedUi(): {
  attachUi: StackNode;
  attachStart: StackNode;
  nodes: StackNode[];
} {
  const attachProto = aStackNode({
    nodeId: "attach-proto",
    title: "Start-session attachment proto",
    branch: ATTACH_PROTO_BRANCH,
    prStatus: { phase: "open" },
  });
  const attachStore = aStackNode({
    nodeId: "attach-store",
    title: "Session attachment storage and context docs",
    branch: ATTACH_STORE_BRANCH,
    prStatus: { phase: "open" },
  });
  const attachStart = aStackNode({
    nodeId: "attach-start",
    title: "Copy attachments during StartSession",
    branchSuggestion: "feature/session-attach-docs/attach-start",
    parents: ["attach-proto", "attach-store"],
  });
  // Repointed onto master: the daemon dropped every parent edge (`RepointPlannedPr`).
  const attachUi = aStackNode({
    nodeId: "attach-ui",
    title: "Create Session attach UI",
    branchSuggestion: "feature/session-attach-docs/attach-ui",
    parents: [],
  });
  return { attachUi, attachStart, nodes: [attachProto, attachStore, attachStart, attachUi] };
}

describe("baseBranchChoice", () => {
  it("pre-selects the project default branch for a planned PR repointed onto it", () => {
    // Given — "Create Session attach UI" repointed onto master, beside two materialized stack branches.
    const { attachUi, nodes } = anAttachDocsStackWithRepointedUi();

    // When
    const choice = baseBranchChoice(attachUi, nodes, DEFAULT_BRANCH);

    // Then — master, not the first stack branch the resolver happens to list.
    expect(choice.selected).toEqual(DEFAULT_BRANCH);
  });

  it("offers the project default branch after the stack's own branches", () => {
    // Given
    const { attachUi, nodes } = anAttachDocsStackWithRepointedUi();

    // When
    const choice = baseBranchChoice(attachUi, nodes, DEFAULT_BRANCH);

    // Then — the stack's materialized branches first, the project default last.
    expect(choice.options).toEqual([
      { value: ATTACH_PROTO_BRANCH, label: ATTACH_PROTO_BRANCH },
      { value: ATTACH_STORE_BRANCH, label: ATTACH_STORE_BRANCH },
      { value: DEFAULT_BRANCH, label: DEFAULT_BRANCH },
    ]);
  });

  it("pre-selects the dependency branch for a planned PR that still depends on a materialized parent", () => {
    // Given — `attach-start`, which still depends on [attach-proto, attach-store]; both are roots, so
    // the first in `parents` wins.
    const { attachStart, nodes } = anAttachDocsStackWithRepointedUi();

    // When
    const choice = baseBranchChoice(attachStart, nodes, DEFAULT_BRANCH);

    // Then — the dependency, not the project default: a node with a predecessor still chains onto it.
    expect(choice.selected).toEqual(ATTACH_PROTO_BRANCH);
  });

  it("offers nothing to choose for a lone planned root", () => {
    // Given — a single planned root: the stack owns no branch anywhere.
    const loneRoot = aStackNode({ nodeId: "n1", title: "root", branchSuggestion: "feature/stack/n1" });

    // When
    const choice = baseBranchChoice(loneRoot, [loneRoot], DEFAULT_BRANCH);

    // Then — no options and no selection: the dialog hides the picker and the daemon resolves the
    // default base itself from an empty `selected_integration_base_ref`.
    expect(choice).toEqual({ options: [], selected: "" });
  });

  it("offers the empty project default under a naming label when the project stores no default branch", () => {
    // Given — a legacy project with no `main_branch_ref` (D20), beside materialized stack branches.
    const { attachUi, nodes } = anAttachDocsStackWithRepointedUi();

    // When
    const choice = baseBranchChoice(attachUi, nodes, "");

    // Then — the empty ref the daemon resolves for itself is still offered, under a label that names it
    // rather than an option that reads blank.
    expect(choice.options[2]).toEqual({ value: "", label: "project default" });
  });

  it("pre-selects the empty project default when the project stores no default branch", () => {
    // Given — the same legacy project.
    const { attachUi, nodes } = anAttachDocsStackWithRepointedUi();

    // When
    const choice = baseBranchChoice(attachUi, nodes, "");

    // Then — the repointed node still starts on the project default, as the empty ref.
    expect(choice.selected).toEqual("");
  });

  /**
   * n3 depends on the merged n2, whose own parent n1 is an open PR owning a branch: the base walks past
   * the merged node to n1, which the option list also carries as an "other" stack branch.
   */
  function aStackWhoseBaseIsAboveAMergedParent(): { n3: StackNode; nodes: StackNode[] } {
    const n1 = aStackNode({ nodeId: "n1", title: "root", branch: "feature/stack/n1", prStatus: { phase: "open" } });
    const n2 = aStackNode({
      nodeId: "n2",
      title: "merged mid",
      branch: "feature/stack/n2",
      prStatus: { phase: "merged" },
      parents: ["n1"],
    });
    const n3 = aStackNode({
      nodeId: "n3",
      title: "planned top",
      branchSuggestion: "feature/stack/n3",
      parents: ["n2"],
    });
    return { n3, nodes: [n1, n2, n3] };
  }

  it("pre-selects the transitive ancestor's branch when the direct parent has merged", () => {
    // Given
    const { n3, nodes } = aStackWhoseBaseIsAboveAMergedParent();

    // When
    const choice = baseBranchChoice(n3, nodes, DEFAULT_BRANCH);

    // Then — n1, reached past the merged n2 whose own `origin` ref may already be gone.
    expect(choice.selected).toEqual("feature/stack/n1");
  });

  it("offers the transitive ancestor's branch it pre-selects when the direct parent has merged", () => {
    // Given
    const { n3, nodes } = aStackWhoseBaseIsAboveAMergedParent();

    // When
    const choice = baseBranchChoice(n3, nodes, DEFAULT_BRANCH);

    // Then — the pre-selection is one of the options, so what is submitted is always something the
    // operator can see selected. The merged n2 is offered by nobody.
    expect(choice.options.map((option) => option.value)).toEqual([
      "feature/stack/n1",
      DEFAULT_BRANCH,
    ]);
  });

  // ---------------------------------------------------------------------------
  // A child of a parent whose PR was merged externally: the parent is branchless and its
  // `pr_status.phase` is still `"open"` (the daemon never merged it, and the branch was deleted on
  // merge). The stack offers no branch of its own, but this is not a lone planned root — the node
  // has a parent — so the project default must be offered as the escape, and pre-selected, so the
  // operator sees the base rather than guessing whether the spawn lands on a stack branch or master.
  // ---------------------------------------------------------------------------

  function aStackWhoseParentWasMergedExternally(): { child: StackNode; nodes: StackNode[] } {
    const bottom = aStackNode({
      nodeId: "bottom",
      title: "bottom",
      branchSuggestion: "feature/stack/bottom",
      sessionId: "child-bottom",
      prStatus: { phase: "open" },
    });
    const child = aStackNode({
      nodeId: "child",
      title: "child",
      branchSuggestion: "feature/stack/child",
      parents: ["bottom"],
    });
    return { child, nodes: [bottom, child] };
  }

  it("offers the project default as the sole option for a child of an externally merged branchless parent", () => {
    // Given — `bottom` was merged externally: branchless, `pr_status.phase` still `"open"`.
    const { child, nodes } = aStackWhoseParentWasMergedExternally();

    // When
    const choice = baseBranchChoice(child, nodes, DEFAULT_BRANCH);

    // Then — the picker renders with the project default as the only escape, so the operator can
    // choose it instead of being left with no base to pick.
    expect(choice.options).toEqual([{ value: DEFAULT_BRANCH, label: DEFAULT_BRANCH }]);
  });

  it("pre-selects the project default for a child of an externally merged branchless parent", () => {
    // Given
    const { child, nodes } = aStackWhoseParentWasMergedExternally();

    // When
    const choice = baseBranchChoice(child, nodes, DEFAULT_BRANCH);

    // Then — the selection names the ref the spawn will branch from, so the operator reads the
    // base before confirming rather than submitting a guess.
    expect(choice.selected).toEqual(DEFAULT_BRANCH);
  });
});

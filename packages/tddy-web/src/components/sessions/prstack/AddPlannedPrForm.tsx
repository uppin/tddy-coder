import React, { useState } from "react";
import { Button } from "../../ui/button";
import { topoSortStackNodes, type StackNode } from "./stackPlan";

export interface AddPlannedPrFormSubmission {
  title: string;
  description: string;
  branchSuggestion: string;
  parents: string[];
}

export interface AddPlannedPrFormProps {
  /** Existing planned-PR nodes offered as ancestor checkboxes, in topo order. */
  nodes: StackNode[];
  /** Rejects to report a failed submission — the form surfaces the error and stays open. */
  onSubmit: (input: AddPlannedPrFormSubmission) => Promise<void>;
  onCancel: () => void;
}

const inputClass =
  "rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

/** Form for manually adding a single planned PR to the stack, with a multi-select ancestor picker. */
export function AddPlannedPrForm({ nodes, onSubmit, onCancel }: AddPlannedPrFormProps) {
  const ordered = topoSortStackNodes(nodes);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [branchSuggestion, setBranchSuggestion] = useState("");
  const [checkedParents, setCheckedParents] = useState<Set<string>>(new Set());
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggleParent = (nodeId: string) => {
    setCheckedParents((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) {
        next.delete(nodeId);
      } else {
        next.add(nodeId);
      }
      return next;
    });
  };

  const handleSubmit = async () => {
    const trimmedTitle = title.trim();
    if (!trimmedTitle) return;
    setSubmitting(true);
    setError(null);
    try {
      await onSubmit({
        title: trimmedTitle,
        description,
        branchSuggestion,
        parents: ordered.filter((n) => checkedParents.has(n.nodeId)).map((n) => n.nodeId),
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      data-testid="pr-stack-add-planned-pr-form"
      className="flex-shrink-0 flex flex-col gap-2 border-b border-border p-3"
    >
      {error && (
        <p
          data-testid="pr-stack-add-planned-pr-error"
          role="alert"
          className="text-xs text-destructive"
        >
          {error}
        </p>
      )}
      <input
        data-testid="pr-stack-add-planned-pr-title-input"
        className={inputClass}
        placeholder="Title"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
      />
      <input
        data-testid="pr-stack-add-planned-pr-description-input"
        className={inputClass}
        placeholder="Description (optional)"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
      />
      <input
        data-testid="pr-stack-add-planned-pr-branch-suggestion-input"
        className={inputClass}
        placeholder="Branch suggestion (optional)"
        value={branchSuggestion}
        onChange={(e) => setBranchSuggestion(e.target.value)}
      />
      {ordered.length > 0 && (
        <div className="flex flex-col gap-1">
          <span className="text-xs font-medium text-muted-foreground">Ancestors</span>
          {ordered.map((node) => (
            <label
              key={node.nodeId}
              data-testid={`pr-stack-add-planned-pr-ancestor-${node.nodeId}`}
              className="flex items-center gap-2 text-sm"
            >
              <input
                type="checkbox"
                checked={checkedParents.has(node.nodeId)}
                onChange={() => toggleParent(node.nodeId)}
              />
              {node.title}
            </label>
          ))}
        </div>
      )}
      <div className="flex gap-2">
        <Button
          data-testid="pr-stack-add-planned-pr-submit-btn"
          size="sm"
          disabled={submitting}
          onClick={handleSubmit}
        >
          Add planned PR
        </Button>
        <Button
          data-testid="pr-stack-add-planned-pr-cancel-btn"
          size="sm"
          variant="outline"
          disabled={submitting}
          onClick={onCancel}
        >
          Cancel
        </Button>
      </div>
    </div>
  );
}

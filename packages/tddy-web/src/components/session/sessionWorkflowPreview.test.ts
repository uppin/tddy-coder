import { describe, expect, it } from "bun:test";
import { workflowPreviewKind } from "./sessionWorkflowPreview";

describe("workflowPreviewKind", () => {
  it("classifies a YAML changeset file as a yaml preview", () => {
    // When
    const result = workflowPreviewKind("changeset.yaml");
    // Then
    expect(result).toBe("yaml");
  });

  it("classifies a Markdown PRD file as a markdown preview", () => {
    // When
    const result = workflowPreviewKind("PRD.md");
    // Then
    expect(result).toBe("markdown");
  });
});

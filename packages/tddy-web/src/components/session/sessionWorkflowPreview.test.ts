import { describe, expect, test } from "bun:test";
import { workflowPreviewKind } from "./sessionWorkflowPreview";

describe("workflowPreviewKind", () => {
  test("classifies changeset.yaml as yaml preview", () => {
    expect(workflowPreviewKind("changeset.yaml")).toBe("yaml");
  });

  test("classifies PRD.md as markdown preview", () => {
    expect(workflowPreviewKind("PRD.md")).toBe("markdown");
  });
});

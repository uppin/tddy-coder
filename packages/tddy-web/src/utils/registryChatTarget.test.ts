/**
 * Unit tests for the Models & Agents chat target: which registry row a chat is opened against, and
 * whether it needs a workspace before it can be opened at all.
 *
 * Changeset: `CS-2026-08-16-models-and-assistants`
 * PRD: `docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md` § ACP chat
 */

import { describe, expect, it } from "bun:test";
import { ModelLoadState } from "../gen/models_pb";
import type { AssistantRow, ModelRow } from "./mergeRegistryEntries";
import { chatWithAssistant, chatWithModel, needsWorkspace } from "./registryChatTarget";

const WORKSTATION = "workstation-1";

function aModelRow(overrides: Partial<ModelRow> = {}): ModelRow {
  return {
    daemonInstanceId: WORKSTATION,
    providerId: "prov-ollama",
    modelId: "qwen3:32b",
    label: "Qwen3 32B",
    labels: ["llm", "tools"],
    loadState: ModelLoadState.LOADED,
    sizeBytes: 20_000_000_000n,
    ...overrides,
  };
}

function anAssistantRow(overrides: Partial<AssistantRow> = {}): AssistantRow {
  return {
    daemonInstanceId: WORKSTATION,
    assistantId: "asst-1",
    name: "repo-reader",
    label: "Repo Reader",
    providerId: "prov-ollama",
    modelId: "qwen3:32b",
    systemPrompt: "You read code.",
    tools: ["Read", "Grep"],
    replaces: [],
    ...overrides,
  };
}

describe("chatWithModel", () => {
  it("names the model's provider and model, and runs no tools", () => {
    // Given
    const model = aModelRow({ providerId: "prov-ollama", modelId: "qwen3:32b" });

    // When
    const target = chatWithModel(model);

    // Then
    expect(target).toEqual({
      daemonInstanceId: WORKSTATION,
      label: "Qwen3 32B",
      modelId: "qwen3:32b",
      providerId: "prov-ollama",
      assistantId: "",
      cwd: "",
    });
  });
});

describe("chatWithAssistant", () => {
  it("names the assistant alone, so the daemon's own record decides what it is", () => {
    // Given
    const assistant = anAssistantRow({ assistantId: "asst-7", providerId: "prov-ollama" });

    // When
    const target = chatWithAssistant(assistant, "/home/dev/project");

    // Then — no provider is claimed from the browser's copy of the registry
    expect(target).toEqual({
      daemonInstanceId: WORKSTATION,
      label: "Repo Reader",
      modelId: "qwen3:32b",
      providerId: "",
      assistantId: "asst-7",
      cwd: "/home/dev/project",
    });
  });

  it("is served by the assistant's owning daemon, not the selected one", () => {
    // Given
    const assistant = anAssistantRow({ daemonInstanceId: "server-2" });

    // When
    const target = chatWithAssistant(assistant, "/srv/project");

    // Then
    expect(target.daemonInstanceId).toEqual("server-2");
  });
});

describe("needsWorkspace", () => {
  it("requires one for an assistant that has tools to run", () => {
    expect(needsWorkspace(anAssistantRow({ tools: ["Read"] }))).toEqual(true);
  });

  it("requires none for an assistant that reaches no tool engine", () => {
    expect(needsWorkspace(anAssistantRow({ tools: [] }))).toEqual(false);
  });
});

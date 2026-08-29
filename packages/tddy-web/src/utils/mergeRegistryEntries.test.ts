/**
 * Unit tests for the Models & Agents cross-daemon merge: owning-daemon attribution and the union of
 * every daemon's registry, including how a daemon that could not be read is reported.
 *
 * Changeset: `CS-2026-08-16-models-and-assistants`
 * PRD: `docs/ft/web/1-WIP/PRD-2026-08-16-models-and-assistants.md` § AC2, AC12
 */

import { describe, it, expect } from "bun:test";
import { ModelLoadState, ProviderKind } from "../gen/models_pb";
import {
  assistantRowKey,
  describeReadFailures,
  mergeRegistryEntries,
  modelRowKey,
  owningDaemonOf,
  providerRowKey,
  registryEmptyStateText,
  registryReadStatus,
  type AssistantRow,
  type DaemonRegistrySnapshot,
  type ModelRow,
  type ProviderRow,
} from "./mergeRegistryEntries";

const WORKSTATION = "workstation-1";
const SERVER = "server-2";

function aProviderRow(overrides: Partial<ProviderRow> = {}): ProviderRow {
  return {
    daemonInstanceId: WORKSTATION,
    providerId: "prov-ollama",
    kind: ProviderKind.OLLAMA,
    label: "Local Ollama",
    baseUrl: "http://localhost:11434",
    hasCredential: false,
    enumerationError: "",
    ...overrides,
  };
}

function aModelRow(overrides: Partial<ModelRow> = {}): ModelRow {
  return {
    daemonInstanceId: WORKSTATION,
    providerId: "prov-ollama",
    modelId: "qwen3:32b",
    label: "Qwen3 32B",
    labels: ["llm", "tools"],
    loadState: ModelLoadState.NOT_LOADED,
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
    systemPrompt: "You read code and answer questions about it.",
    tools: ["Read", "Grep"],
    replaces: [],
    ...overrides,
  };
}

function aSnapshot(overrides: Partial<DaemonRegistrySnapshot> = {}): DaemonRegistrySnapshot {
  return {
    instanceId: WORKSTATION,
    providers: [aProviderRow()],
    models: [aModelRow()],
    assistants: [anAssistantRow()],
    error: "",
    ...overrides,
  };
}

describe("owningDaemonOf", () => {
  it("keeps the instance id the serving daemon stamped on the row", () => {
    // Given / When
    const owner = owningDaemonOf(SERVER, WORKSTATION);

    // Then
    expect(owner).toEqual(SERVER);
  });

  it("attributes an unstamped row to the daemon it was read from", () => {
    // Given / When
    const owner = owningDaemonOf("", WORKSTATION);

    // Then
    expect(owner).toEqual(WORKSTATION);
  });
});

describe("mergeRegistryEntries", () => {
  it("lists both daemons' models in daemon order", () => {
    // Given
    const snapshots = [
      aSnapshot({ instanceId: WORKSTATION, models: [aModelRow()] }),
      aSnapshot({
        instanceId: SERVER,
        models: [aModelRow({ daemonInstanceId: SERVER, modelId: "llama3.3:70b" })],
      }),
    ];

    // When
    const merged = mergeRegistryEntries(snapshots);

    // Then
    expect(merged.models.map(modelRowKey)).toEqual([
      `${WORKSTATION}/prov-ollama/qwen3:32b`,
      `${SERVER}/prov-ollama/llama3.3:70b`,
    ]);
  });

  it("reports a daemon that could not be read as a failure and contributes none of its rows", () => {
    // Given — a daemon none of whose lists arrived, so it has nothing to contribute but the reason
    const snapshots = [
      aSnapshot({ instanceId: WORKSTATION }),
      aSnapshot({
        instanceId: SERVER,
        providers: [],
        models: [],
        assistants: [],
        error: "common room peer is unreachable",
      }),
    ];

    // When
    const merged = mergeRegistryEntries(snapshots);

    // Then
    expect(merged.failures).toEqual([
      { instanceId: SERVER, error: "common room peer is unreachable" },
    ]);
    expect(merged.models.map(modelRowKey)).toEqual([`${WORKSTATION}/prov-ollama/qwen3:32b`]);
    expect(merged.providers.map((p) => p.daemonInstanceId)).toEqual([WORKSTATION]);
    expect(merged.assistants.map((a) => a.daemonInstanceId)).toEqual([WORKSTATION]);
  });

  it("lists the rows a partly failed daemon did answer with, alongside its failure", () => {
    // Given — a daemon whose assistants could not be read, but whose models could
    const snapshots = [
      aSnapshot({
        instanceId: SERVER,
        providers: [aProviderRow({ daemonInstanceId: SERVER })],
        models: [aModelRow({ daemonInstanceId: SERVER })],
        assistants: [],
        error: "assistants: no SubagentTool variant for tool 'Sleep'",
      }),
    ];

    // When
    const merged = mergeRegistryEntries(snapshots);

    // Then
    expect(merged.failures).toEqual([
      { instanceId: SERVER, error: "assistants: no SubagentTool variant for tool 'Sleep'" },
    ]);
    expect(merged.models.map(modelRowKey)).toEqual([`${SERVER}/prov-ollama/qwen3:32b`]);
    expect(merged.assistants).toEqual([]);
  });

  it("reports no failures when every daemon answered", () => {
    // Given
    const snapshots = [aSnapshot({ instanceId: WORKSTATION }), aSnapshot({ instanceId: SERVER })];

    // When
    const merged = mergeRegistryEntries(snapshots);

    // Then
    expect(merged.failures).toEqual([]);
  });
});

describe("providerRowKey", () => {
  it("distinguishes two daemons' providers that were minted with the same id", () => {
    // Given — every host running Ollama has a provider called `prov-ollama`
    const onWorkstation = { daemonInstanceId: WORKSTATION, providerId: "prov-ollama" };
    const onServer = { daemonInstanceId: SERVER, providerId: "prov-ollama" };

    // When / Then
    expect(providerRowKey(onWorkstation)).toEqual(`${WORKSTATION}/prov-ollama`);
    expect(providerRowKey(onServer)).toEqual(`${SERVER}/prov-ollama`);
  });
});

describe("assistantRowKey", () => {
  it("distinguishes two daemons' assistants that were given the same name", () => {
    // Given — an assistant name is unique per daemon, so both hosts may define a `reviewer`
    const onWorkstation = { daemonInstanceId: WORKSTATION, name: "reviewer" };
    const onServer = { daemonInstanceId: SERVER, name: "reviewer" };

    // When / Then
    expect(assistantRowKey(onWorkstation)).toEqual(`${WORKSTATION}/reviewer`);
    expect(assistantRowKey(onServer)).toEqual(`${SERVER}/reviewer`);
  });
});

describe("registryEmptyStateText", () => {
  const PROVIDERS = { loading: "Reading the fleet's providers…", ready: "No providers configured" };

  it("states the panel's own claim once every daemon has answered", () => {
    // Given / When / Then
    expect(registryEmptyStateText("ready", PROVIDERS)).toEqual("No providers configured");
  });

  it("states what is being read while a daemon has yet to answer", () => {
    // Given / When / Then
    expect(registryEmptyStateText("loading", PROVIDERS)).toEqual("Reading the fleet's providers…");
  });

  it("reports a room that is not connected rather than the panel's own claim", () => {
    // Given / When / Then
    expect(registryEmptyStateText("not-connected", PROVIDERS)).toEqual(
      "Not connected to the common room",
    );
  });

  it("reports a room with no daemons rather than the panel's own claim", () => {
    // Given / When / Then
    expect(registryEmptyStateText("no-daemons", PROVIDERS)).toEqual(
      "No daemons in the common room",
    );
  });
});

describe("describeReadFailures", () => {
  it("is empty when every list arrived", () => {
    // Given / When / Then
    expect(describeReadFailures([])).toEqual("");
  });

  it("names the one list that failed and the daemon's words for why", () => {
    // Given
    const failures = [{ list: "assistants", message: "no SubagentTool variant for tool 'Sleep'" }];

    // When / Then
    expect(describeReadFailures(failures)).toEqual(
      "assistants: no SubagentTool variant for tool 'Sleep'",
    );
  });

  it("states one shared reason once, listing the lists it cost", () => {
    // Given — an unreachable daemon fails all four reads with the same message
    const failures = ["providers", "models", "assistants", "tools"].map((list) => ({
      list,
      message: "common room peer is unreachable",
    }));

    // When / Then
    expect(describeReadFailures(failures)).toEqual(
      "providers, models, assistants, tools: common room peer is unreachable",
    );
  });

  it("keeps distinct reasons apart, in the order they were reported", () => {
    // Given
    const failures = [
      { list: "models", message: "deadline exceeded" },
      { list: "assistants", message: "permission denied" },
    ];

    // When / Then
    expect(describeReadFailures(failures)).toEqual(
      "models: deadline exceeded; assistants: permission denied",
    );
  });
});

describe("registryReadStatus", () => {
  it("is not-connected while there is no common-room connection", () => {
    // Given / When / Then
    expect(registryReadStatus({ connected: false, daemonCount: 0, answeredCount: 0 })).toEqual(
      "not-connected",
    );
  });

  it("is no-daemons when the room is connected but holds no daemon", () => {
    // Given / When / Then
    expect(registryReadStatus({ connected: true, daemonCount: 0, answeredCount: 0 })).toEqual(
      "no-daemons",
    );
  });

  it("is loading while a daemon has yet to answer", () => {
    // Given / When / Then
    expect(registryReadStatus({ connected: true, daemonCount: 2, answeredCount: 1 })).toEqual(
      "loading",
    );
  });

  it("is ready once every daemon has answered", () => {
    // Given / When / Then
    expect(registryReadStatus({ connected: true, daemonCount: 2, answeredCount: 2 })).toEqual(
      "ready",
    );
  });
});

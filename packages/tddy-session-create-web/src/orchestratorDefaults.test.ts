/**
 * Unit tests for reading and writing a project's orchestrator defaults (`#orchestrators` 11/11):
 * the composer reads the row for its project **on its host** — defaults are per host — and the
 * settings pane sends none at all when every control is back at the agent's own default.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-project-settings-pane.md
 */

import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { OrchestratorDefaultsSchema, ProjectEntrySchema } from "tddy-client-web/gen/project_pb";
import {
  NO_ORCHESTRATOR_DEFAULTS,
  orchestratorDefaultsFor,
  orchestratorDefaultsMessage,
  type ProjectDefaultsRow,
} from "./orchestratorDefaults";

const BETA = "proj-beta";
const HOST_A = "workstation-1";
const HOST_B = "server-2";

/** `projectId`'s row on `host`, with `defaults` saved (none when omitted). */
function aRow(
  projectId: string,
  host: string,
  defaults?: { model?: string; effort?: string; providerAccountId?: string },
): ProjectDefaultsRow {
  return create(ProjectEntrySchema, {
    projectId,
    daemonInstanceId: host,
    orchestratorDefaults: defaults ? create(OrchestratorDefaultsSchema, defaults) : undefined,
  });
}

describe("orchestratorDefaultsFor", () => {
  it("reads the defaults saved for the project on the composer's host", () => {
    // Given
    const projects = [aRow(BETA, HOST_A, { model: "sonnet", effort: "high", providerAccountId: "acct-meta" })];

    // When
    const choice = orchestratorDefaultsFor(projects, BETA, HOST_A);

    // Then
    expect(choice).toEqual({ model: "sonnet", effort: "high", providerAccountId: "acct-meta" });
  });

  it("reads the composer host's row, not another host's for the same project", () => {
    // Given
    const projects = [
      aRow(BETA, HOST_B, { model: "opus", effort: "low" }),
      aRow(BETA, HOST_A, { model: "sonnet" }),
    ];

    // When
    const choice = orchestratorDefaultsFor(projects, BETA, HOST_A);

    // Then
    expect(choice).toEqual({ model: "sonnet", effort: "", providerAccountId: "" });
  });

  it("is nothing saved when the host's row has no defaults", () => {
    // When
    const choice = orchestratorDefaultsFor([aRow(BETA, HOST_A)], BETA, HOST_A);

    // Then
    expect(choice).toEqual(NO_ORCHESTRATOR_DEFAULTS);
  });

  it("is nothing saved when the host does not list the project", () => {
    // When
    const choice = orchestratorDefaultsFor([aRow(BETA, HOST_B, { model: "opus" })], BETA, HOST_A);

    // Then
    expect(choice).toEqual(NO_ORCHESTRATOR_DEFAULTS);
  });
});

describe("orchestratorDefaultsMessage", () => {
  it("carries every chosen control", () => {
    // When
    const message = orchestratorDefaultsMessage({ model: "sonnet", effort: "high", providerAccountId: "acct-meta" });

    // Then
    expect(message).toEqual({ model: "sonnet", effort: "high", providerAccountId: "acct-meta" });
  });

  it("carries a partly chosen set, the rest left at the agent's default", () => {
    // When
    const message = orchestratorDefaultsMessage({ model: "", effort: "high", providerAccountId: "" });

    // Then
    expect(message).toEqual({ model: "", effort: "high", providerAccountId: "" });
  });

  it("is absent — a clear — when every control is at the agent's default", () => {
    // When
    const message = orchestratorDefaultsMessage(NO_ORCHESTRATOR_DEFAULTS);

    // Then
    expect(message).toBeUndefined();
  });
});

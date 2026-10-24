/**
 * Unit tests for the drawer's project-home slotting: which groups exist for a pinned **main**
 * entry, and which sessions a group still lists as rows.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry (`#project-home` 4/7).
 * PRD: docs/ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md — main is the session
 * the project records (`homeSessionId`), not whichever session carries the role.
 */

import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { ProjectEntrySchema } from "../gen/project_pb";
import { SessionEntrySchema } from "../gen/session_pb";
import {
  projectHomeSlot,
  projectHomesOnHost,
  withProjectHomeGroups,
  PROJECT_HOME_ROLE,
} from "./projectHomeGroups";
import type { ProjectSessionGroup } from "./sessionProjectGroups";

const ALPHA = { projectId: "p-alpha", name: "alpha" };
const BETA = { projectId: "p-beta", name: "Beta" };
const GAMMA = { projectId: "p-gamma", name: "gamma" };

const aSession = (sessionId: string, projectId: string, sessionRole = "") =>
  create(SessionEntrySchema, { sessionId, projectId, sessionRole });

const aGroup = (project: { projectId: string; name: string }, ...ids: string[]): ProjectSessionGroup => ({
  projectId: project.projectId,
  label: project.name,
  sessions: ids.map((id) => aSession(id, project.projectId)),
});

const OTHER: ProjectSessionGroup = { projectId: null, label: "Other", sessions: [aSession("o1", "")] };

const keys = (groups: ProjectSessionGroup[]) => groups.map((g) => g.projectId);

describe("withProjectHomeGroups", () => {
  it("adds an empty group for a project whose main is not listed, in name order, before Other", () => {
    const groups = withProjectHomeGroups(
      [aGroup(ALPHA, "a1"), aGroup(GAMMA, "g1"), OTHER],
      [ALPHA, BETA, GAMMA],
      [{ projectId: BETA.projectId, homeSessionId: "home-b" }],
    );

    expect(keys(groups)).toEqual([ALPHA.projectId, BETA.projectId, GAMMA.projectId, null]);
    expect(groups[1].sessions).toEqual([]);
  });

  it("adds nothing for a project without a main", () => {
    const input = [aGroup(ALPHA, "a1")];

    const groups = withProjectHomeGroups(input, [ALPHA, BETA], [
      { projectId: BETA.projectId, homeSessionId: null },
    ]);

    expect(groups).toBe(input);
  });

  it("does not duplicate a project that already has a group", () => {
    const groups = withProjectHomeGroups([aGroup(ALPHA, "a1")], [ALPHA], [
      { projectId: ALPHA.projectId, homeSessionId: "h" },
    ]);

    expect(keys(groups)).toEqual([ALPHA.projectId]);
  });
});

describe("projectHomeSlot", () => {
  const HOME = { projectId: ALPHA.projectId, mainBranchRef: "origin/main", homeSessionId: "h" };

  it("lifts the recorded main out of the rows", () => {
    const home = aSession("h", ALPHA.projectId, PROJECT_HOME_ROLE);
    const group = { ...aGroup(ALPHA, "a1"), sessions: [home, aSession("a1", ALPHA.projectId)] };

    const slot = projectHomeSlot(group, [HOME]);

    expect(slot.home).toEqual(HOME);
    expect(slot.homeSession?.sessionId).toBe("h");
    expect(slot.rows.map((s) => s.sessionId)).toEqual(["a1"]);
  });

  it("keeps a project-home session that is not the recorded main as a row", () => {
    // Given a replaced main beside the recorded one, both carrying the role
    const replaced = aSession("old", ALPHA.projectId, PROJECT_HOME_ROLE);
    const main = aSession("h", ALPHA.projectId, PROJECT_HOME_ROLE);
    const group = { ...aGroup(ALPHA), sessions: [replaced, main] };

    // When
    const slot = projectHomeSlot(group, [HOME]);

    // Then
    expect(slot.homeSession?.sessionId).toBe("h");
    expect(slot.rows.map((s) => s.sessionId)).toEqual(["old"]);
  });

  it("leaves the Other group with every session as a row and no pinned entry", () => {
    const slot = projectHomeSlot(OTHER, [HOME]);

    expect(slot).toEqual({ home: undefined, homeSession: null, rows: OTHER.sessions });
  });
});

describe("projectHomesOnHost", () => {
  const aProjectRow = (projectId: string, daemonInstanceId: string, homeSessionId: string) =>
    create(ProjectEntrySchema, { projectId, daemonInstanceId, homeSessionId, mainBranchRef: "origin/main" });

  it("reads each project's main from the row of the host main is ensured on", () => {
    // Given
    const rows = [aProjectRow(ALPHA.projectId, "host-a", ""), aProjectRow(ALPHA.projectId, "host-b", "home-b")];

    // When
    const homes = projectHomesOnHost(rows, "host-b");

    // Then
    expect(homes).toEqual([{ projectId: ALPHA.projectId, mainBranchRef: "origin/main", homeSessionId: "home-b" }]);
  });

  it("reads an empty home session id as main not set up", () => {
    // Given
    const rows = [aProjectRow(ALPHA.projectId, "host-a", "")];

    // When
    const homes = projectHomesOnHost(rows, "host-a");

    // Then
    expect(homes).toEqual([{ projectId: ALPHA.projectId, mainBranchRef: "origin/main", homeSessionId: null }]);
  });

  it("gives a project with no row on that host no home", () => {
    // Given
    const rows = [aProjectRow(BETA.projectId, "host-a", "home-a")];

    // When
    const homes = projectHomesOnHost(rows, "host-b");

    // Then
    expect(homes).toEqual([]);
  });

  it("gives no homes while no host is selected", () => {
    // Given
    const rows = [aProjectRow(ALPHA.projectId, "host-a", "home-a")];

    // When
    const homes = projectHomesOnHost(rows, null);

    // Then
    expect(homes).toEqual([]);
  });
});

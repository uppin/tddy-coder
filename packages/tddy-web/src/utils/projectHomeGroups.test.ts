/**
 * Unit tests for the drawer's project-home slotting: which groups exist for a pinned **main**
 * entry, and which sessions a group still lists as rows.
 *
 * Feature doc: docs/ft/web/session-drawer.md § Pinned main entry (`#project-home` 4/7).
 */

import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema } from "../gen/session_pb";
import { projectHomeSlot, withProjectHomeGroups, PROJECT_HOME_ROLE } from "./projectHomeGroups";
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
  it("adds an empty group for a project with a home agent and no session, in name order, before Other", () => {
    const groups = withProjectHomeGroups(
      [aGroup(ALPHA, "a1"), aGroup(GAMMA, "g1"), OTHER],
      [ALPHA, BETA, GAMMA],
      [{ projectId: BETA.projectId, homeAgentLabel: "Claude" }],
    );

    expect(keys(groups)).toEqual([ALPHA.projectId, BETA.projectId, GAMMA.projectId, null]);
    expect(groups[1].sessions).toEqual([]);
  });

  it("adds nothing for a project without a home agent", () => {
    const input = [aGroup(ALPHA, "a1")];

    const groups = withProjectHomeGroups(input, [ALPHA, BETA], [
      { projectId: BETA.projectId, homeAgentLabel: null },
    ]);

    expect(groups).toBe(input);
  });

  it("does not duplicate a project that already has a group", () => {
    const groups = withProjectHomeGroups([aGroup(ALPHA, "a1")], [ALPHA], [
      { projectId: ALPHA.projectId, homeAgentLabel: "Claude" },
    ]);

    expect(keys(groups)).toEqual([ALPHA.projectId]);
  });
});

describe("projectHomeSlot", () => {
  const HOME = { projectId: ALPHA.projectId, mainBranchRef: "origin/main", homeAgentLabel: "Claude" };

  it("lifts the project-home session out of the rows", () => {
    const home = aSession("h", ALPHA.projectId, PROJECT_HOME_ROLE);
    const group = { ...aGroup(ALPHA, "a1"), sessions: [home, aSession("a1", ALPHA.projectId)] };

    const slot = projectHomeSlot(group, [HOME]);

    expect(slot.home).toEqual(HOME);
    expect(slot.homeSession?.sessionId).toBe("h");
    expect(slot.rows.map((s) => s.sessionId)).toEqual(["a1"]);
  });

  it("leaves the Other group with every session as a row and no pinned entry", () => {
    const slot = projectHomeSlot(OTHER, [HOME]);

    expect(slot).toEqual({ home: undefined, homeSession: null, rows: OTHER.sessions });
  });
});

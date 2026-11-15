/**
 * Unit tests for per-project grouping of the sessions drawer.
 *
 * PRD: docs/ft/web/session-drawer.md (`#project-home` 2/7).
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "tddy-client-web/gen/session_pb";
import { ProjectEntrySchema } from "tddy-client-web/gen/project_pb";
import {
  groupSessionsByProject,
  splitDeletedProjects,
  withoutDeletedProjectsSessions,
  type ProjectSessionGroup,
} from "./sessionProjectGroups";

function aSession(sessionId: string, projectId: string): SessionEntry {
  return create(SessionEntrySchema, { sessionId, projectId });
}

const ZETA = { projectId: "proj-zeta", name: "zeta" };
const ALPHA = { projectId: "proj-alpha", name: "Alpha" };
const BETA = { projectId: "proj-beta", name: "beta" };

/** `[label, [session ids]]` per group, in order — what a reader of the drawer sees. */
function shapeOf(groups: ProjectSessionGroup[]): [string, string[]][] {
  return groups.map((g) => [g.label, g.sessions.map((s) => s.sessionId)]);
}

describe("groupSessionsByProject", () => {
  it("groups sessions under their project's name, ordered by name", () => {
    // Given
    const sessions = [aSession("z1", ZETA.projectId), aSession("a1", ALPHA.projectId), aSession("b1", BETA.projectId), aSession("a2", ALPHA.projectId)];

    // When
    const groups = groupSessionsByProject(sessions, [ZETA, ALPHA, BETA]);

    // Then
    expect(shapeOf(groups)).toEqual([
      ["Alpha", ["a1", "a2"]],
      ["beta", ["b1"]],
      ["zeta", ["z1"]],
    ]);
  });

  it("puts project-less and unknown-project sessions in a trailing Other group", () => {
    // Given
    const sessions = [aSession("none", ""), aSession("a1", ALPHA.projectId), aSession("ghost", "proj-unknown")];

    // When
    const groups = groupSessionsByProject(sessions, [ALPHA]);

    // Then
    expect(shapeOf(groups)).toEqual([
      ["Alpha", ["a1"]],
      ["Other", ["none", "ghost"]],
    ]);
    expect(groups[1].projectId).toBeNull();
  });

  it("omits the Other group when every session has a known project", () => {
    // Given
    const sessions = [aSession("a1", ALPHA.projectId)];

    // When
    const groups = groupSessionsByProject(sessions, [ALPHA, BETA]);

    // Then
    expect(shapeOf(groups)).toEqual([["Alpha", ["a1"]]]);
  });
});

describe("deleted projects in the drawer", () => {
  const live = create(ProjectEntrySchema, { projectId: ALPHA.projectId, name: ALPHA.name, daemonInstanceId: "host-a" });
  const deletedOnA = create(ProjectEntrySchema, {
    projectId: BETA.projectId,
    name: BETA.name,
    daemonInstanceId: "host-a",
    deletedAt: "2026-11-15T10:00:00.000Z",
  });
  const deletedOnB = create(ProjectEntrySchema, { ...deletedOnA, daemonInstanceId: "host-b" });

  it("splits a listing into the live projects and the ids of the deleted ones", () => {
    // When
    const { live: liveRows, deletedProjectIds } = splitDeletedProjects([live, deletedOnA, deletedOnB]);

    // Then
    expect(liveRows.map((p) => p.projectId)).toEqual([ALPHA.projectId]);
    expect([...deletedProjectIds]).toEqual([BETA.projectId]);
  });

  it("drops the sessions of a deleted project instead of moving them to Other", () => {
    // Given
    const sessions = [aSession("a1", ALPHA.projectId), aSession("b1", BETA.projectId), aSession("o1", "")];

    // When
    const kept = withoutDeletedProjectsSessions(sessions, new Set([BETA.projectId]));

    // Then
    expect(kept.map((s) => s.sessionId)).toEqual(["a1", "o1"]);
  });
});

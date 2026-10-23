/**
 * Unit tests for per-project grouping of the sessions drawer.
 *
 * PRD: docs/ft/web/session-drawer.md (`#project-home` 2/7).
 */

import { describe, it, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema, type SessionEntry } from "../gen/session_pb";
import { groupSessionsByProject, type ProjectSessionGroup } from "./sessionProjectGroups";

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

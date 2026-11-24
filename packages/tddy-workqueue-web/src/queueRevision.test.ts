import { describe, expect, test } from "bun:test";
import { revisionOf, supersedes } from "./queueRevision";

describe("revisionOf", () => {
  test("reads the revision a queue carries", () => {
    expect(revisionOf(JSON.stringify({ version: 1, nodes: [], revision: 7 }))).toBe(7);
  });

  test("is 0 for a queue that carries none, an empty answer, or one that is not JSON", () => {
    expect([revisionOf(JSON.stringify({ version: 1, nodes: [] })), revisionOf(""), revisionOf("{nope")]).toEqual([
      0, 0, 0,
    ]);
  });
});

describe("supersedes", () => {
  test("the first copy is always taken", () => {
    expect([supersedes(null, 3), supersedes(null, 0)]).toEqual([true, true]);
  });

  test("a newer copy replaces the one held", () => {
    expect(supersedes(3, 4)).toBe(true);
  });

  test("a copy no newer than the one held is ignored", () => {
    expect([supersedes(4, 4), supersedes(4, 3)]).toEqual([false, false]);
  });

  test("an unnumbered copy is taken as it comes", () => {
    expect([supersedes(4, 0), supersedes(0, 0)]).toEqual([true, true]);
  });
});

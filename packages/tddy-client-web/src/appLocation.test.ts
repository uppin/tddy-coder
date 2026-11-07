import { describe, expect, it } from "bun:test";
import {
  formatAppLocation,
  parseAppLocation,
  screenRootOf,
  withParams,
  withPath,
  type AppLocation,
} from "./appLocation";

/** A location builder — tests override only the field the scenario is about. */
function aLocation(overrides: Partial<AppLocation> = {}): AppLocation {
  return { path: "/sessions", params: {}, ...overrides };
}

describe("parseAppLocation", () => {
  it("reads a bare path with no params", () => {
    // When
    const location = parseAppLocation("#/sessions");

    // Then
    expect(location).toEqual(aLocation({ path: "/sessions" }));
  });

  it("splits the hash-local query string off the path", () => {
    // When
    const location = parseAppLocation("#/sessions/abc?host=laptop-b&inspector=files");

    // Then
    expect(location).toEqual(
      aLocation({ path: "/sessions/abc", params: { host: "laptop-b", inspector: "files" } }),
    );
  });

  it("reads an empty hash as the root path", () => {
    // When
    const location = parseAppLocation("");

    // Then
    expect(location).toEqual(aLocation({ path: "/" }));
  });

  it("tolerates a hash without the leading '#'", () => {
    // When
    const location = parseAppLocation("/tasks/task-1?channel=2");

    // Then
    expect(location).toEqual(aLocation({ path: "/tasks/task-1", params: { channel: "2" } }));
  });

  it("percent-decodes a param value", () => {
    // When
    const location = parseAppLocation("#/worktrees?project=proj%2Falpha");

    // Then
    expect(location.params.project).toBe("proj/alpha");
  });

  it("reads a param with an empty value as an empty string", () => {
    // When
    const location = parseAppLocation("#/sessions?host=");

    // Then
    expect(location.params.host).toBe("");
  });
});

describe("formatAppLocation", () => {
  it("formats a bare path as a hash with no query string", () => {
    // When
    const hash = formatAppLocation(aLocation({ path: "/sessions" }));

    // Then
    expect(hash).toBe("#/sessions");
  });

  it("appends the params as a hash-local query string", () => {
    // When
    const hash = formatAppLocation(
      aLocation({ path: "/sessions/abc", params: { host: "udoo", code: "1" } }),
    );

    // Then
    expect(hash).toBe("#/sessions/abc?host=udoo&code=1");
  });

  it("percent-encodes a param value that contains a reserved character", () => {
    // When
    const hash = formatAppLocation(aLocation({ path: "/worktrees", params: { project: "a/b" } }));

    // Then
    expect(hash).toBe("#/worktrees?project=a%2Fb");
  });

  it("round-trips a location through format and parse", () => {
    // Given
    const original = aLocation({
      path: "/sessions/9f3c-0001",
      params: { host: "laptop-b", inspector: "worktree", full: "1" },
    });

    // When
    const roundTripped = parseAppLocation(formatAppLocation(original));

    // Then
    expect(roundTripped).toEqual(original);
  });
});

describe("withParams", () => {
  it("adds a param that was not present", () => {
    // Given
    const location = aLocation({ path: "/sessions/abc" });

    // When
    const next = withParams(location, { inspector: "tools" });

    // Then
    expect(next).toEqual(aLocation({ path: "/sessions/abc", params: { inspector: "tools" } }));
  });

  it("overwrites a param that was already present", () => {
    // Given
    const location = aLocation({ path: "/sessions/abc", params: { inspector: "details" } });

    // When
    const next = withParams(location, { inspector: "files" });

    // Then
    expect(next.params).toEqual({ inspector: "files" });
  });

  it("deletes a param when its patch value is null", () => {
    // Given
    const location = aLocation({
      path: "/sessions/abc",
      params: { host: "udoo", inspector: "details" },
    });

    // When
    const next = withParams(location, { inspector: null });

    // Then
    expect(next.params).toEqual({ host: "udoo" });
  });

  it("applies several param changes in one patch", () => {
    // Given
    const location = aLocation({
      path: "/sessions/abc",
      params: { host: "udoo", inspector: "details", full: "1" },
    });

    // When
    const next = withParams(location, { inspector: "usage", full: null, code: "1" });

    // Then
    expect(next.params).toEqual({ host: "udoo", inspector: "usage", code: "1" });
  });

  it("leaves the original location untouched", () => {
    // Given
    const location = aLocation({ path: "/sessions/abc", params: { host: "udoo" } });

    // When
    withParams(location, { host: "laptop-b" });

    // Then
    expect(location.params).toEqual({ host: "udoo" });
  });
});

/**
 * `withPath` distinguishes a **screen change** (a different first path segment) from a move *within*
 * a screen: screen-scoped params are meaningless on another screen, but must survive a move between
 * two sessions — the inspector does not close because the operator clicked the next row.
 */
describe("withPath", () => {
  it("carries the host across a screen change", () => {
    // Given — the sessions screen, on a named host, with a session selected
    const location = aLocation({
      path: "/sessions/abc",
      params: { host: "laptop-b", inspector: "details" },
    });

    // When — the operator picks another screen from the hamburger menu
    const next = withPath(location, "/tasks");

    // Then — the host survives; the sessions-screen params do not
    expect(next).toEqual(aLocation({ path: "/tasks", params: { host: "laptop-b" } }));
  });

  it("drops every screen-scoped param on a screen change when no host is set", () => {
    // Given
    const location = aLocation({ path: "/tasks/task-1", params: { channel: "2" } });

    // When
    const next = withPath(location, "/projects");

    // Then
    expect(next).toEqual(aLocation({ path: "/projects", params: {} }));
  });

  it("keeps the params when moving between two sessions on the same screen", () => {
    // Given
    const location = aLocation({
      path: "/sessions/abc",
      params: { host: "udoo", inspector: "worktree", code: "1" },
    });

    // When
    const next = withPath(location, "/sessions/def");

    // Then
    expect(next).toEqual(
      aLocation({
        path: "/sessions/def",
        params: { host: "udoo", inspector: "worktree", code: "1" },
      }),
    );
  });

  it("keeps the params when the path is unchanged", () => {
    // Given
    const location = aLocation({ path: "/sessions/abc", params: { code: "1", host: "udoo" } });

    // When
    const next = withPath(location, "/sessions/abc");

    // Then
    expect(next.params).toEqual({ code: "1", host: "udoo" });
  });
});

describe("screenRootOf", () => {
  it("strips a sub-selection back to the screen root", () => {
    // When
    const result = screenRootOf("/sessions/abc");

    // Then
    expect(result).toBe("/sessions");
  });

  it("strips a trailing mode segment back to the screen root", () => {
    // When
    const result = screenRootOf("/sessions/abc/add-agent");

    // Then
    expect(result).toBe("/sessions");
  });

  it("leaves a path that is already a screen root unchanged", () => {
    // When
    const result = screenRootOf("/worktrees");

    // Then
    expect(result).toBe("/worktrees");
  });

  it("maps the root path to itself", () => {
    // When
    const result = screenRootOf("/");

    // Then
    expect(result).toBe("/");
  });
});

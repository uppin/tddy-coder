/**
 * `resolveSelectedDaemonInstanceId` precedence: a choice (URL, then sessionStorage) is kept whether
 * or not that daemon is listed; only without one does it default to the serving daemon, then the
 * first listed daemon, then none.
 *
 * PRD: docs/ft/web/url-state-routing.md § Host in the URL.
 *
 * The resolution rule moves out of `rpc/selectedDaemon.tsx` into this pure module: importing it
 * from the `.tsx` provider drags React's JSX runtime into a pure unit test, which is why the
 * existing `src/rpc/selectedDaemon.test.ts` cannot run under `bun test` today (and is excluded from
 * every `bun test` path in `package.json`).
 */

import { describe, expect, it } from "bun:test";
import type { DaemonHost } from "../lib/participantRole";
import { withMockedSessionStorage } from "../test-utils/sessionStorage";
import {
  readStoredSelectedDaemon,
  resolveSelectedDaemonInstanceId,
  writeStoredSelectedDaemon,
} from "./selectedHost";

const UDOO: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };
const LAPTOP_B: DaemonHost = { instanceId: "laptop-b", label: "laptop-b" };
const BOTH = [UDOO, LAPTOP_B];

// ---------------------------------------------------------------------------
// Precedence rules 2–5: the stored choice, then the defaults. Relocated from
// `src/rpc/selectedDaemon.test.ts`, which could not run: it imported a `.tsx` module.
// ---------------------------------------------------------------------------

describe("resolveSelectedDaemonInstanceId — without a URL host", () => {
  it("defaults to the serving daemon when nothing is stored", () => {
    // Given / When
    const selected = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: null,
    });

    // Then
    expect(selected).toBe("udoo");
  });

  it("prefers the stored selection over the serving daemon", () => {
    // Given / When
    const selected = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "laptop-b",
    });

    // Then
    expect(selected).toBe("laptop-b");
  });

  it("keeps the stored selection when it has left the room", () => {
    // Given — the previously selected peer is no longer in the common room
    const selected = resolveSelectedDaemonInstanceId({
      daemons: [UDOO],
      servingInstanceId: "udoo",
      storedInstanceId: "laptop-b",
    });

    // Then — a choice is kept; the selector waits for it
    expect(selected).toBe("laptop-b");
  });

  it("falls back to the first available daemon when nothing is chosen and the serving daemon is not listed", () => {
    // Given — this web session's own serving daemon has itself dropped off the common room
    const selected = resolveSelectedDaemonInstanceId({
      daemons: [LAPTOP_B],
      servingInstanceId: "udoo",
      storedInstanceId: null,
    });

    // Then
    expect(selected).toBe("laptop-b");
  });

  it("keeps the stored selection when nothing is listed", () => {
    // Given / When — the directory has not heard from any source yet
    const selected = resolveSelectedDaemonInstanceId({
      daemons: [],
      servingInstanceId: "udoo",
      storedInstanceId: "laptop-b",
    });

    // Then
    expect(selected).toBe("laptop-b");
  });

  it("returns null when nothing is chosen and no daemons are listed", () => {
    // Given / When
    const selected = resolveSelectedDaemonInstanceId({
      daemons: [],
      servingInstanceId: "udoo",
      storedInstanceId: null,
    });

    // Then
    expect(selected).toBeNull();
  });
});

describe("selected-daemon session storage", () => {
  it("round-trips a selection through session storage", () => {
    withMockedSessionStorage(() => {
      // Given / When
      writeStoredSelectedDaemon("laptop-b");

      // Then
      expect(readStoredSelectedDaemon()).toBe("laptop-b");
    });
  });

  it("returns null when nothing has been stored yet", () => {
    withMockedSessionStorage(() => {
      // Given / When / Then — fresh tab (empty sessionStorage)
      expect(readStoredSelectedDaemon()).toBeNull();
    });
  });
});

describe("resolveSelectedDaemonInstanceId — URL precedence", () => {
  it("prefers the URL host over the stored one", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "udoo",
      urlInstanceId: "laptop-b",
    });

    // Then
    expect(result).toBe("laptop-b");
  });

  it("prefers the URL host over the serving daemon when nothing is stored", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: null,
      urlInstanceId: "laptop-b",
    });

    // Then
    expect(result).toBe("laptop-b");
  });

  it("keeps the URL host when it is not listed", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "laptop-b",
      urlInstanceId: "retired-host",
    });

    // Then
    expect(result).toBe("retired-host");
  });

  it("keeps the URL host over the stored one when neither is listed", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "retired-store-host",
      urlInstanceId: "retired-url-host",
    });

    // Then
    expect(result).toBe("retired-url-host");
  });

  it("keeps the URL host even when the directory is empty", () => {
    // When — the page has not heard from any source yet
    const result = resolveSelectedDaemonInstanceId({
      daemons: [],
      servingInstanceId: "udoo",
      storedInstanceId: null,
      urlInstanceId: "laptop-b",
    });

    // Then
    expect(result).toBe("laptop-b");
  });

  it("treats an empty URL host as no choice", () => {
    // When — `?host=` is present but names nothing
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: null,
      urlInstanceId: "",
    });

    // Then — the serving daemon is the default, as with no URL host at all
    expect(result).toBe("udoo");
  });

  it("uses the stored host when the URL names none", () => {
    // When
    const result = resolveSelectedDaemonInstanceId({
      daemons: BOTH,
      servingInstanceId: "udoo",
      storedInstanceId: "laptop-b",
      urlInstanceId: null,
    });

    // Then
    expect(result).toBe("laptop-b");
  });
});

/**
 * The labels this tab has seen daemons listed under, so a selected daemon that is not listed right
 * now — the common room still joining after a reload, or the daemon gone — is shown by a name the
 * operator recognises instead of a bare instance id.
 *
 * PRD: docs/ft/web/url-state-routing.md § Host in the URL.
 */

import { describe, expect, it } from "bun:test";
import type { DaemonHost } from "../lib/participantRole";
import { withMockedSessionStorage } from "../test-utils/sessionStorage";
import {
  REMEMBERED_DAEMON_LABELS_STORAGE_KEY,
  readRememberedDaemonLabel,
  rememberDaemonLabels,
} from "./selectedHost";

const UDOO: DaemonHost = { instanceId: "udoo", label: "udoo (this daemon)" };
const LAPTOP_B: DaemonHost = { instanceId: "laptop-b", label: "Laptop B" };

describe("remembered daemon labels", () => {
  it("remembers the label of every listed daemon", () => {
    withMockedSessionStorage(() => {
      // Given / When
      rememberDaemonLabels([UDOO, LAPTOP_B]);

      // Then
      expect(readRememberedDaemonLabel("laptop-b")).toBe("Laptop B");
    });
  });

  it("returns null for a daemon this tab has never listed", () => {
    withMockedSessionStorage(() => {
      // Given
      rememberDaemonLabels([UDOO]);

      // When / Then
      expect(readRememberedDaemonLabel("laptop-b")).toBeNull();
    });
  });

  it("keeps a daemon's label after it drops out of the list", () => {
    withMockedSessionStorage(() => {
      // Given — laptop-b was listed once
      rememberDaemonLabels([UDOO, LAPTOP_B]);

      // When — a later listing no longer has it
      rememberDaemonLabels([UDOO]);

      // Then
      expect(readRememberedDaemonLabel("laptop-b")).toBe("Laptop B");
    });
  });

  it("takes the newer label when a daemon re-advertises under another one", () => {
    withMockedSessionStorage(() => {
      // Given
      rememberDaemonLabels([LAPTOP_B]);

      // When
      rememberDaemonLabels([{ instanceId: "laptop-b", label: "Laptop B (renamed)" }]);

      // Then
      expect(readRememberedDaemonLabel("laptop-b")).toBe("Laptop B (renamed)");
    });
  });

  it("reads nothing from a stored value that is not a label map", () => {
    withMockedSessionStorage(() => {
      // Given
      sessionStorage.setItem(REMEMBERED_DAEMON_LABELS_STORAGE_KEY, "not json");

      // When / Then
      expect(readRememberedDaemonLabel("laptop-b")).toBeNull();
    });
  });

  it("replaces a stored value that is not a label map when remembering", () => {
    withMockedSessionStorage(() => {
      // Given
      sessionStorage.setItem(REMEMBERED_DAEMON_LABELS_STORAGE_KEY, "not json");

      // When
      rememberDaemonLabels([LAPTOP_B]);

      // Then
      expect(readRememberedDaemonLabel("laptop-b")).toBe("Laptop B");
    });
  });

  it("reads nothing from stored JSON that is not an object", () => {
    withMockedSessionStorage(() => {
      // Given
      sessionStorage.setItem(REMEMBERED_DAEMON_LABELS_STORAGE_KEY, "[]");

      // When / Then
      expect(readRememberedDaemonLabel("laptop-b")).toBeNull();
    });
  });

  it("reads nothing from stored JSON whose labels are not strings", () => {
    withMockedSessionStorage(() => {
      // Given
      sessionStorage.setItem(REMEMBERED_DAEMON_LABELS_STORAGE_KEY, '{"labels":{"laptop-b":42}}');

      // When / Then
      expect(readRememberedDaemonLabel("laptop-b")).toBeNull();
    });
  });

  it("returns null for an instance id that names an object built-in", () => {
    withMockedSessionStorage(() => {
      // Given
      rememberDaemonLabels([LAPTOP_B]);

      // When / Then — `?host=toString` is an unlisted host, not `Object.prototype.toString`
      expect(readRememberedDaemonLabel("toString")).toBeNull();
    });
  });

  it("keeps what it remembered when a listing is empty", () => {
    withMockedSessionStorage(() => {
      // Given
      rememberDaemonLabels([LAPTOP_B]);

      // When
      rememberDaemonLabels([]);

      // Then
      expect(readRememberedDaemonLabel("laptop-b")).toBe("Laptop B");
    });
  });
});

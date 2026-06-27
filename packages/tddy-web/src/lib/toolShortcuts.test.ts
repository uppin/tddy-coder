import { describe, expect, it } from "bun:test";
import {
  keySequenceToBytes,
  toolIdentifierFromPath,
  resolveShortcutsForSession,
  TOOL_SHORTCUTS,
} from "./toolShortcuts";

describe("keySequenceToBytes", () => {
  it("encodes Tab as 0x09", () => {
    expect(keySequenceToBytes(["Tab"])).toEqual(new Uint8Array([0x09]));
  });

  it("encodes Shift+Tab as the reverse-tab sequence ESC [ Z", () => {
    expect(keySequenceToBytes(["Shift", "Tab"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x5a]),
    );
  });

  it("encodes Escape as 0x1b", () => {
    expect(keySequenceToBytes(["Escape"])).toEqual(new Uint8Array([0x1b]));
  });

  it("encodes Enter as 0x0d", () => {
    expect(keySequenceToBytes(["Enter"])).toEqual(new Uint8Array([0x0d]));
  });

  it("encodes Backspace as 0x7f", () => {
    expect(keySequenceToBytes(["Backspace"])).toEqual(new Uint8Array([0x7f]));
  });

  it("encodes Delete as ESC [ 3 ~", () => {
    expect(keySequenceToBytes(["Delete"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x33, 0x7e]),
    );
  });

  it("encodes ArrowUp as ESC [ A", () => {
    expect(keySequenceToBytes(["ArrowUp"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x41]),
    );
  });

  it("encodes ArrowDown as ESC [ B", () => {
    expect(keySequenceToBytes(["ArrowDown"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x42]),
    );
  });

  it("encodes ArrowRight as ESC [ C", () => {
    expect(keySequenceToBytes(["ArrowRight"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x43]),
    );
  });

  it("encodes ArrowLeft as ESC [ D", () => {
    expect(keySequenceToBytes(["ArrowLeft"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x44]),
    );
  });

  it("encodes Home as ESC [ H", () => {
    expect(keySequenceToBytes(["Home"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x48]),
    );
  });

  it("encodes End as ESC [ F", () => {
    expect(keySequenceToBytes(["End"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x46]),
    );
  });

  it("encodes PageUp as ESC [ 5 ~", () => {
    expect(keySequenceToBytes(["PageUp"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x35, 0x7e]),
    );
  });

  it("encodes PageDown as ESC [ 6 ~", () => {
    expect(keySequenceToBytes(["PageDown"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x36, 0x7e]),
    );
  });

  it("encodes Ctrl+C as 0x03", () => {
    expect(keySequenceToBytes(["Ctrl", "C"])).toEqual(new Uint8Array([0x03]));
  });

  it("encodes Ctrl+R as 0x12", () => {
    expect(keySequenceToBytes(["Ctrl", "R"])).toEqual(new Uint8Array([0x12]));
  });

  it("encodes Ctrl+Z as 0x1a", () => {
    expect(keySequenceToBytes(["Ctrl", "Z"])).toEqual(new Uint8Array([0x1a]));
  });

  it("encodes a single printable character as UTF-8", () => {
    expect(keySequenceToBytes(["q"])).toEqual(new Uint8Array([0x71]));
  });

  it("returns empty Uint8Array for an unrecognized key", () => {
    expect(keySequenceToBytes(["F99"])).toEqual(new Uint8Array(0));
  });

  it("returns empty Uint8Array for Ctrl with a non-letter key", () => {
    expect(keySequenceToBytes(["Ctrl", "Tab"])).toEqual(new Uint8Array(0));
  });

  it("encodes Alt+M as ESC m", () => {
    expect(keySequenceToBytes(["Alt", "M"])).toEqual(new Uint8Array([0x1b, 0x6d]));
  });

  it("returns empty Uint8Array for Alt with a non-letter key", () => {
    expect(keySequenceToBytes(["Alt", "Tab"])).toEqual(new Uint8Array(0));
  });

  it("encodes F1 as ESC O P", () => {
    expect(keySequenceToBytes(["F1"])).toEqual(
      new Uint8Array([0x1b, 0x4f, 0x50]),
    );
  });

  it("encodes F4 as ESC O S", () => {
    expect(keySequenceToBytes(["F4"])).toEqual(
      new Uint8Array([0x1b, 0x4f, 0x53]),
    );
  });

  it("encodes F5 as ESC [ 1 5 ~", () => {
    expect(keySequenceToBytes(["F5"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x31, 0x35, 0x7e]),
    );
  });

  it("encodes F12 as ESC [ 2 4 ~", () => {
    expect(keySequenceToBytes(["F12"])).toEqual(
      new Uint8Array([0x1b, 0x5b, 0x32, 0x34, 0x7e]),
    );
  });
});

describe("toolIdentifierFromPath", () => {
  it("returns 'tddy-coder' for debug build path", () => {
    expect(toolIdentifierFromPath("target/debug/tddy-coder")).toBe("tddy-coder");
  });

  it("returns 'tddy-coder' for release build path", () => {
    expect(toolIdentifierFromPath("target/release/tddy-coder")).toBe("tddy-coder");
  });

  it("returns 'tddy-tools' for tddy-tools debug path", () => {
    expect(toolIdentifierFromPath("target/debug/tddy-tools")).toBe("tddy-tools");
  });

  it("returns 'tddy-tools' for tddy-tools release path", () => {
    expect(toolIdentifierFromPath("target/release/tddy-tools")).toBe("tddy-tools");
  });

  it("returns 'default' for an unrecognized tool path", () => {
    expect(toolIdentifierFromPath("target/debug/some-other-tool")).toBe("default");
  });

  it("returns 'default' for an empty string", () => {
    expect(toolIdentifierFromPath("")).toBe("default");
  });

  it("returns 'tddy-coder' when path has additional segments", () => {
    expect(toolIdentifierFromPath("/home/user/project/target/debug/tddy-coder")).toBe(
      "tddy-coder",
    );
  });
});

describe("resolveShortcutsForSession", () => {
  it("returns claude-cli shortcuts for a claude-cli session", () => {
    const result = resolveShortcutsForSession(true, "");
    expect(result).toBe(TOOL_SHORTCUTS["claude-cli"]);
  });

  it("returns tddy-coder shortcuts for a managed tddy-coder session", () => {
    const result = resolveShortcutsForSession(false, "target/debug/tddy-coder");
    expect(result).toBe(TOOL_SHORTCUTS["tddy-coder"]);
  });

  it("returns default shortcuts for an unknown tool path", () => {
    const result = resolveShortcutsForSession(false, "");
    expect(result).toBe(TOOL_SHORTCUTS["default"] ?? []);
  });

  it("returns an empty array when the default shortcut list is empty", () => {
    const result = resolveShortcutsForSession(false, "");
    expect(Array.isArray(result)).toBe(true);
    expect(result.length).toBe(0);
  });
});

describe("TOOL_SHORTCUTS", () => {
  it("provides at least one shortcut for tddy-coder", () => {
    expect((TOOL_SHORTCUTS["tddy-coder"] ?? []).length).toBeGreaterThan(0);
  });

  it("provides at least one shortcut for claude-cli", () => {
    expect((TOOL_SHORTCUTS["claude-cli"] ?? []).length).toBeGreaterThan(0);
  });

  it("each shortcut has a non-empty label and a non-empty keys array", () => {
    for (const [, shortcuts] of Object.entries(TOOL_SHORTCUTS)) {
      for (const s of shortcuts) {
        expect(s.label.length).toBeGreaterThan(0);
        expect(s.keys.length).toBeGreaterThan(0);
      }
    }
  });
});

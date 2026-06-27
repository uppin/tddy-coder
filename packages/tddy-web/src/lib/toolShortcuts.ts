export interface ToolShortcutDef {
  label: string;
  keys: string[];
}

export const TOOL_SHORTCUTS: Record<string, ToolShortcutDef[]> = {
  "tddy-coder": [
    { label: "Shift+Tab", keys: ["Shift", "Tab"] },
    { label: "Ctrl+C", keys: ["Ctrl", "C"] },
    { label: "Escape", keys: ["Escape"] },
  ],
  "claude-cli": [
    { label: "Shift+Tab", keys: ["Shift", "Tab"] },
    { label: "Alt+M", keys: ["Alt", "M"] },
    { label: "Escape", keys: ["Escape"] },
    { label: "Ctrl+R", keys: ["Ctrl", "R"] },
    { label: "Ctrl+C", keys: ["Ctrl", "C"] },
  ],
  "default": [],
};

const NAMED_KEY_SEQUENCES: Record<string, Uint8Array> = {
  Tab: new Uint8Array([0x09]),
  Escape: new Uint8Array([0x1b]),
  Enter: new Uint8Array([0x0d]),
  Backspace: new Uint8Array([0x7f]),
  Delete: new Uint8Array([0x1b, 0x5b, 0x33, 0x7e]),
  ArrowUp: new Uint8Array([0x1b, 0x5b, 0x41]),
  ArrowDown: new Uint8Array([0x1b, 0x5b, 0x42]),
  ArrowRight: new Uint8Array([0x1b, 0x5b, 0x43]),
  ArrowLeft: new Uint8Array([0x1b, 0x5b, 0x44]),
  Home: new Uint8Array([0x1b, 0x5b, 0x48]),
  End: new Uint8Array([0x1b, 0x5b, 0x46]),
  PageUp: new Uint8Array([0x1b, 0x5b, 0x35, 0x7e]),
  PageDown: new Uint8Array([0x1b, 0x5b, 0x36, 0x7e]),
  // F1–F4: SS3 sequences
  F1: new Uint8Array([0x1b, 0x4f, 0x50]),
  F2: new Uint8Array([0x1b, 0x4f, 0x51]),
  F3: new Uint8Array([0x1b, 0x4f, 0x52]),
  F4: new Uint8Array([0x1b, 0x4f, 0x53]),
  // F5–F12: CSI sequences
  F5: new Uint8Array([0x1b, 0x5b, 0x31, 0x35, 0x7e]),
  F6: new Uint8Array([0x1b, 0x5b, 0x31, 0x37, 0x7e]),
  F7: new Uint8Array([0x1b, 0x5b, 0x31, 0x38, 0x7e]),
  F8: new Uint8Array([0x1b, 0x5b, 0x31, 0x39, 0x7e]),
  F9: new Uint8Array([0x1b, 0x5b, 0x32, 0x30, 0x7e]),
  F10: new Uint8Array([0x1b, 0x5b, 0x32, 0x31, 0x7e]),
  F11: new Uint8Array([0x1b, 0x5b, 0x32, 0x33, 0x7e]),
  F12: new Uint8Array([0x1b, 0x5b, 0x32, 0x34, 0x7e]),
};

const SHIFT_TAB = new Uint8Array([0x1b, 0x5b, 0x5a]);

export function keySequenceToBytes(keys: string[]): Uint8Array {
  if (keys.length === 2 && keys[0] === "Shift" && keys[1] === "Tab") {
    return SHIFT_TAB;
  }

  if (keys.length === 2 && keys[0] === "Ctrl") {
    const letter = keys[1];
    if (letter.length === 1) {
      const lower = letter.toLowerCase();
      const code = lower.charCodeAt(0);
      if (code >= 0x61 && code <= 0x7a) {
        return new Uint8Array([code - 96]);
      }
    }
    return new Uint8Array(0);
  }

  // Alt+letter → meta-sends-escape: ESC followed by the lowercased letter (e.g. Alt+M → ESC m).
  if (keys.length === 2 && keys[0] === "Alt") {
    const letter = keys[1];
    if (letter.length === 1) {
      const lower = letter.toLowerCase();
      const code = lower.charCodeAt(0);
      if (code >= 0x61 && code <= 0x7a) {
        return new Uint8Array([0x1b, code]);
      }
    }
    return new Uint8Array(0);
  }

  if (keys.length === 1) {
    const key = keys[0]!;
    const named = NAMED_KEY_SEQUENCES[key];
    if (named) return named;
    if (key.length === 1) {
      return new TextEncoder().encode(key);
    }
    return new Uint8Array(0);
  }

  return new Uint8Array(0);
}

export function toolIdentifierFromPath(toolPath: string): string {
  const basename = toolPath.split("/").pop() ?? "";
  if (basename.includes("tddy-coder")) return "tddy-coder";
  if (basename.includes("tddy-tools")) return "tddy-tools";
  return "default";
}

export function resolveShortcutsForSession(
  isClaudeCli: boolean,
  toolPath: string,
): ToolShortcutDef[] {
  if (isClaudeCli) return TOOL_SHORTCUTS["claude-cli"] ?? [];
  const id = toolIdentifierFromPath(toolPath);
  return TOOL_SHORTCUTS[id] ?? TOOL_SHORTCUTS["default"] ?? [];
}

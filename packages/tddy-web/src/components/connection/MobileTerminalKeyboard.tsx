import React from "react";

export interface MobileTerminalKeyboardProps {
  /** Forward terminal input (raw text or pre-encoded bytes) to the PTY. */
  onSend: (data: string | Uint8Array) => void;
  /** Override the label styling (defaults to the inline keyboard button look). */
  className?: string;
}

const DEFAULT_LABEL_CLASS =
  "relative inline-flex shrink-0 cursor-pointer items-center rounded border border-input bg-background px-3 py-1 text-xs text-foreground";

/**
 * Hidden-input mobile keyboard affordance.
 *
 * Soft keyboards fire DOM `input` events (not `keydown` with a `key`), so the
 * terminal's `onData` never sees them. This transparent input captures both:
 * `onInput` forwards typed characters, and `onKeyDown` maps the special keys
 * (Ctrl+letter, Enter, Backspace, Tab, Escape, arrows) to their control bytes.
 */
export function MobileTerminalKeyboard({ onSend, className }: MobileTerminalKeyboardProps) {
  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    // Ctrl+letter must send control bytes (e.g. Ctrl+C → 0x03). Otherwise `onInput` only sees "c"
    // (0x63) — the same bug as VirtualTui view_state swallowing Ctrl as text.
    if (e.ctrlKey && e.key.length === 1) {
      const lower = e.key.toLowerCase();
      if (lower >= "a" && lower <= "z") {
        e.preventDefault();
        onSend(new Uint8Array([lower.charCodeAt(0) - 96]));
        return;
      }
    }
    const key = e.key;
    if (key === "Enter") {
      e.preventDefault();
      onSend(new Uint8Array([0x0d]));
      return;
    }
    if (key === "Backspace") {
      e.preventDefault();
      onSend(new Uint8Array([0x7f]));
      return;
    }
    if (key === "Tab") {
      e.preventDefault();
      onSend(new Uint8Array([0x09]));
      return;
    }
    if (key === "Escape") {
      e.preventDefault();
      onSend(new Uint8Array([0x1b]));
      return;
    }
    if (key.startsWith("Arrow")) {
      e.preventDefault();
      const seq =
        key === "ArrowUp"
          ? "\x1b[A"
          : key === "ArrowDown"
            ? "\x1b[B"
            : key === "ArrowRight"
              ? "\x1b[C"
              : key === "ArrowLeft"
                ? "\x1b[D"
                : null;
      if (seq) onSend(seq);
    }
  };

  const handleInput = (e: React.FormEvent<HTMLInputElement>) => {
    const input = e.currentTarget;
    const data = (e.nativeEvent as InputEvent).data;
    if (data) onSend(data);
    input.value = "";
  };

  return (
    <label data-testid="mobile-keyboard-button" className={className ?? DEFAULT_LABEL_CLASS}>
      <input
        type="text"
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        aria-label="Open keyboard for terminal input"
        className="absolute inset-0 h-full w-full opacity-0"
        style={{ margin: 0, border: "none", fontSize: 1 }}
        onKeyDown={handleKeyDown}
        onInput={handleInput}
      />
      <span className="pointer-events-none">Keyboard</span>
    </label>
  );
}

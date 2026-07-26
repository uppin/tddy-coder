/**
 * Unit tests for the CLI session-type helpers.
 *
 * The Claude model catalog is not tested here: it lives in tddy-core
 * (`backend::claude_cli_models`) and reaches the web only over `ListAgentModels`, so there is no web
 * constant left to pin. See docs/ft/web/tool-session-model-selection.md.
 *
 * PRD: docs/ft/daemon/claude-cli-session.md
 */

import { describe, expect, it } from "bun:test";

import {
  isCliTerminalSession,
  isClaudeCliSession,
  isCursorCliSession,
} from "../constants/claudeCliModels";

// ---------------------------------------------------------------------------
// isClaudeCliSession
// ---------------------------------------------------------------------------

describe("isClaudeCliSession", () => {
  it("returns true when agent is 'claude-cli'", () => {
    expect(isClaudeCliSession("claude-cli")).toBe(true);
  });

  it("returns false for the 'claude' agent string", () => {
    expect(isClaudeCliSession("claude")).toBe(false);
  });

  it("returns false for the 'claude-acp' agent string", () => {
    expect(isClaudeCliSession("claude-acp")).toBe(false);
  });

  it("returns false for the 'cursor' agent string", () => {
    expect(isClaudeCliSession("cursor")).toBe(false);
  });

  it("returns false for an empty agent string", () => {
    expect(isClaudeCliSession("")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// isCursorCliSession
// ---------------------------------------------------------------------------

describe("isCursorCliSession", () => {
  it("returns true when agent is 'cursor-cli'", () => {
    expect(isCursorCliSession("cursor-cli")).toBe(true);
  });

  it("returns false for the 'cursor' agent string", () => {
    expect(isCursorCliSession("cursor")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// isCliTerminalSession
// ---------------------------------------------------------------------------

describe("isCliTerminalSession", () => {
  it("accepts a claude-cli session", () => {
    expect(isCliTerminalSession("claude-cli")).toBe(true);
  });

  it("accepts a cursor-cli session", () => {
    expect(isCliTerminalSession("cursor-cli")).toBe(true);
  });

  it("rejects a LiveKit-backed tool session", () => {
    expect(isCliTerminalSession("claude")).toBe(false);
  });
});

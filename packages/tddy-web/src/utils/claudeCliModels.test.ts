/**
 * Unit tests for the Claude Code CLI model list constant and session type helpers.
 *
 * PRD: docs/ft/daemon/claude-cli-session.md
 *
 * ALL TESTS CURRENTLY FAIL:
 * - `../constants/claudeCliModels` does not exist yet.
 */

import { describe, expect, it } from "bun:test";

// FAILS: module does not exist yet — implement at packages/tddy-web/src/constants/claudeCliModels.ts
import { CLAUDE_CLI_MODELS, isClaudeCliSession } from "../constants/claudeCliModels";

// ---------------------------------------------------------------------------
// CLAUDE_CLI_MODELS
// ---------------------------------------------------------------------------

describe("CLAUDE_CLI_MODELS", () => {
  it("contains at least one model", () => {
    // FAILS: module not found.
    expect(CLAUDE_CLI_MODELS.length).toBeGreaterThan(0);
  });

  it("all model ids are non-empty strings", () => {
    // FAILS: module not found.
    // Filter-then-length gives a readable failure: the filtered array contains the offenders.
    const entriesWithEmptyId = CLAUDE_CLI_MODELS.filter((m) => m.id === "");
    expect(entriesWithEmptyId).toHaveLength(0);
  });

  it("all model labels are non-empty strings", () => {
    // FAILS: module not found.
    const entriesWithEmptyLabel = CLAUDE_CLI_MODELS.filter((m) => m.label === "");
    expect(entriesWithEmptyLabel).toHaveLength(0);
  });

  it("includes claude-opus-4-8 as a model", () => {
    // FAILS: module not found.
    const ids = CLAUDE_CLI_MODELS.map((m) => m.id);
    expect(ids).toContain("claude-opus-4-8");
  });

  it("includes claude-sonnet-4-6 as a model", () => {
    // FAILS: module not found.
    const ids = CLAUDE_CLI_MODELS.map((m) => m.id);
    expect(ids).toContain("claude-sonnet-4-6");
  });

  it("includes claude-haiku-4-5-20251001 as a model", () => {
    // FAILS: module not found.
    const ids = CLAUDE_CLI_MODELS.map((m) => m.id);
    expect(ids).toContain("claude-haiku-4-5-20251001");
  });

  it("model ids are unique across the list", () => {
    // FAILS: module not found.
    const ids = CLAUDE_CLI_MODELS.map((m) => m.id);
    const unique = new Set(ids);
    expect(unique.size).toBe(ids.length);
  });
});

// ---------------------------------------------------------------------------
// isClaudeCliSession
// ---------------------------------------------------------------------------

describe("isClaudeCliSession", () => {
  it("returns true when agent is 'claude-cli'", () => {
    // FAILS: module not found.
    expect(isClaudeCliSession("claude-cli")).toBe(true);
  });

  it("returns false for the 'claude' agent string", () => {
    // FAILS: module not found.
    expect(isClaudeCliSession("claude")).toBe(false);
  });

  it("returns false for the 'claude-acp' agent string", () => {
    // FAILS: module not found.
    expect(isClaudeCliSession("claude-acp")).toBe(false);
  });

  it("returns false for the 'cursor' agent string", () => {
    // FAILS: module not found.
    expect(isClaudeCliSession("cursor")).toBe(false);
  });

  it("returns false for an empty agent string", () => {
    // FAILS: module not found.
    expect(isClaudeCliSession("")).toBe(false);
  });
});

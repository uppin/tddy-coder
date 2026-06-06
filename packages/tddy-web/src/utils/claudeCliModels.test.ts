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

describe("CLAUDE_CLI_MODELS", () => {
  it("contains at least one model", () => {
    // FAILS: module not found.
    expect(CLAUDE_CLI_MODELS.length).toBeGreaterThan(0);
  });

  it("every entry has a non-empty id and label", () => {
    // FAILS: module not found.
    for (const m of CLAUDE_CLI_MODELS) {
      expect(m.id).toBeTruthy();
      expect(m.label).toBeTruthy();
    }
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

  it("ids are unique", () => {
    // FAILS: module not found.
    const ids = CLAUDE_CLI_MODELS.map((m) => m.id);
    const unique = new Set(ids);
    expect(unique.size).toBe(ids.length);
  });
});

describe("isClaudeCliSession", () => {
  it("returns true when agent is 'claude-cli'", () => {
    // FAILS: module not found.
    expect(isClaudeCliSession("claude-cli")).toBe(true);
  });

  it("returns false for regular agent strings", () => {
    // FAILS: module not found.
    expect(isClaudeCliSession("claude")).toBe(false);
    expect(isClaudeCliSession("claude-acp")).toBe(false);
    expect(isClaudeCliSession("cursor")).toBe(false);
    expect(isClaudeCliSession("")).toBe(false);
  });
});

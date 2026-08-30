import { describe, expect, it } from "bun:test";
import { SessionAgentStatus } from "../../gen/connection_pb";
import {
  agentStatusName,
  agentStatusToken,
  lastActivityText,
  statusIsWorking,
} from "./agentStatusDisplay";

/**
 * The vocabulary both kinds of row in the Agents tab draw their badge from.
 *
 * It is pinned here rather than only through a mounted row because it is now *shared*: a managed
 * roster agent reports `SessionAgentEntry.status` and a non-managed subagent session reports the
 * inferred `SessionEntry.agent_status`, and the proto ships one enum for both precisely so one badge
 * renders them alike. Two renderers wording `EXECUTING_TOOL` differently is the failure this file
 * exists to make impossible.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § The Agents tab (AC53b).
 */

const NOW_MS = 1_780_828_020_298;

describe("agentStatusName", () => {
  it("names a status the daemon has nothing to say about as unknown", () => {
    // "idle" would read as "free, ready for work", which is a claim nobody has made.
    expect(agentStatusName(SessionAgentStatus.UNSPECIFIED)).toEqual("unknown");
  });

  it("names a turn inside a tool call in words rather than in the enum's spelling", () => {
    expect(agentStatusName(SessionAgentStatus.EXECUTING_TOOL)).toEqual("executing tool");
  });

  it("names an agent blocked on a human as waiting for input", () => {
    expect(agentStatusName(SessionAgentStatus.WAITING_FOR_INPUT)).toEqual("waiting for input");
  });

  it("shows a status this build has no name for as itself", () => {
    // A value from a newer daemon must render as the row worth looking at, not be folded into a
    // state it is not.
    expect(agentStatusName(99 as SessionAgentStatus)).toEqual("99");
  });
});

describe("agentStatusToken", () => {
  it("gives a multi-word status one kebab-case token for a selector to match on", () => {
    expect(agentStatusToken(SessionAgentStatus.EXECUTING_TOOL)).toEqual("executing-tool");
  });

  it("gives an unspecified status the unknown token", () => {
    expect(agentStatusToken(SessionAgentStatus.UNSPECIFIED)).toEqual("unknown");
  });
});

describe("statusIsWorking", () => {
  it("counts a turn in flight as working", () => {
    expect(statusIsWorking(SessionAgentStatus.RUNNING)).toEqual(true);
  });

  it("counts an agent blocked on a human as working", () => {
    // It is the row an operator most needs to look at — nothing moves until they answer.
    expect(statusIsWorking(SessionAgentStatus.WAITING_FOR_INPUT)).toEqual(true);
  });

  it("does not count an idle agent as working", () => {
    expect(statusIsWorking(SessionAgentStatus.IDLE)).toEqual(false);
  });

  it("does not count an agent nothing is known about as working", () => {
    expect(statusIsWorking(SessionAgentStatus.UNSPECIFIED)).toEqual(false);
  });
});

describe("lastActivityText", () => {
  it("reads an activity from the last few seconds as just now", () => {
    expect(lastActivityText("answered", BigInt(NOW_MS - 3_000), NOW_MS)).toEqual(
      "answered · just now",
    );
  });

  it("reads an activity from within the minute in seconds", () => {
    expect(lastActivityText("Read src/main.rs", BigInt(NOW_MS - 42_000), NOW_MS)).toEqual(
      "Read src/main.rs · 42s ago",
    );
  });

  it("reads an activity from within the hour in minutes", () => {
    expect(lastActivityText("prompted", BigInt(NOW_MS - 4 * 60_000), NOW_MS)).toEqual(
      "prompted · 4m ago",
    );
  });

  it("reads an activity from within the day in hours", () => {
    expect(lastActivityText("prompted", BigInt(NOW_MS - 3 * 3_600_000), NOW_MS)).toEqual(
      "prompted · 3h ago",
    );
  });

  it("reads an older activity in days", () => {
    expect(lastActivityText("prompted", BigInt(NOW_MS - 2 * 86_400_000), NOW_MS)).toEqual(
      "prompted · 2d ago",
    );
  });

  it("reads a stamp from the future as just now", () => {
    // Two hosts' clocks disagree by seconds routinely, and "in -3s" reads as a bug in the page.
    expect(lastActivityText("prompted", BigInt(NOW_MS + 3_000), NOW_MS)).toEqual(
      "prompted · just now",
    );
  });
});

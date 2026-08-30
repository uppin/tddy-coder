import { describe, expect, it } from "bun:test";
import {
  appendAnswerChunk,
  appendOperatorTurn,
  type AgentTurn,
} from "./agentConversationTranscript";

/**
 * The pure projection behind an agent conversation tab: `AgentConversationChunk` frames folded into
 * turns (docs/ft/web/1-WIP/PRD-2026-08-29-session-agent-conversation-tab.md, AC6-AC8).
 *
 * The daemon's contract is that an answer arrives as one or more frames, only the last of which is
 * marked `last` and carries `stop_reason`, and that an empty answer still yields exactly one frame
 * (`packages/tddy-service/proto/connection.proto:496-505`). Every case below is one clause of that
 * contract, which is why this is a unit test rather than a stream-driven one.
 */

/** A frame as `PromptAgentConversation` yields it. */
function aChunk(contentChunk: string) {
  return { contentChunk, stopReason: "", last: false };
}

/** The final frame of an answer — the only one that carries a stop reason. */
function aFinalChunk(contentChunk: string, stopReason: string) {
  return { contentChunk, stopReason, last: true };
}

/** A transcript holding one sent prompt, which is where every answer starts. */
function afterPrompting(prompt: string): AgentTurn[] {
  return appendOperatorTurn([], prompt);
}

describe("agentConversationTranscript", () => {
  it("records the operator's prompt as a completed turn the moment it is sent", () => {
    // Given an empty transcript
    // When the operator sends a prompt
    const turns = appendOperatorTurn([], "what does foo.rs do?");

    // Then it is one operator turn, complete — a prompt is finished as soon as it leaves
    expect(turns).toEqual([
      { role: "operator", text: "what does foo.rs do?", stopReason: "", complete: true },
    ]);
  });

  it("opens an agent turn on the first answer chunk", () => {
    // Given a sent prompt
    const sent = afterPrompting("what does foo.rs do?");

    // When the first chunk of the answer arrives
    const turns = appendAnswerChunk(sent, aChunk("it parses "));

    // Then a second, still-incomplete turn holds it
    expect(turns).toEqual([
      { role: "operator", text: "what does foo.rs do?", stopReason: "", complete: true },
      { role: "agent", text: "it parses ", stopReason: "", complete: false },
    ]);
  });

  it("concatenates later chunks into the same agent turn", () => {
    // Given an answer that has started arriving
    const started = appendAnswerChunk(afterPrompting("explain"), aChunk("it parses "));

    // When two more chunks arrive
    const turns = appendAnswerChunk(appendAnswerChunk(started, aChunk("the config ")), aChunk("file"));

    // Then they are one turn, not three
    expect(turns.map((t) => t.role)).toEqual(["operator", "agent"]);
    expect(turns[1].text).toEqual("it parses the config file");
  });

  it("completes the agent turn with the stop reason the final frame carries", () => {
    // Given an answer mid-flight
    const started = appendAnswerChunk(afterPrompting("explain"), aChunk("it parses "));

    // When the final frame arrives
    const turns = appendAnswerChunk(started, aFinalChunk("the config file", "EndTurn"));

    // Then the turn is closed and says why it ended
    expect(turns[1]).toEqual({
      role: "agent",
      text: "it parses the config file",
      stopReason: "EndTurn",
      complete: true,
    });
  });

  it("renders an empty answer as one completed agent turn", () => {
    // Given a sent prompt
    const sent = afterPrompting("anything to add?");

    // When the agent answers with nothing — still exactly one frame, per the daemon's contract
    const turns = appendAnswerChunk(sent, aFinalChunk("", "EndTurn"));

    // Then "said nothing" is a turn, never an absence
    expect(turns).toEqual([
      { role: "operator", text: "anything to add?", stopReason: "", complete: true },
      { role: "agent", text: "", stopReason: "EndTurn", complete: true },
    ]);
  });

  it("opens a new agent turn for the answer after a completed one", () => {
    // Given a finished exchange, and a second prompt sent
    const firstExchange = appendAnswerChunk(
      afterPrompting("first question"),
      aFinalChunk("first answer", "EndTurn"),
    );
    const secondSent = appendOperatorTurn(firstExchange, "second question");

    // When the second answer starts arriving
    const turns = appendAnswerChunk(secondSent, aChunk("second answer"));

    // Then it does not extend the first answer
    expect(turns.map((t) => t.text)).toEqual([
      "first question",
      "first answer",
      "second question",
      "second answer",
    ]);
  });

  it("leaves the turns it was given untouched", () => {
    // Given a transcript
    const sent = afterPrompting("explain");

    // When a chunk is folded in
    appendAnswerChunk(sent, aChunk("because"));

    // Then the input is unchanged — the transcript is React state, and mutating it in place would
    // fold a chunk in without ever re-rendering the tab
    expect(sent).toEqual([
      { role: "operator", text: "explain", stopReason: "", complete: true },
    ]);
  });
});

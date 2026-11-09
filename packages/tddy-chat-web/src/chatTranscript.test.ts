import { describe, expect, it } from "bun:test";
import { buildChatTranscript } from "./chatTranscript";
import type { ChatMessage } from "./useAgentChat";

function aMessage(from: ChatMessage["from"], text: string, at: number): ChatMessage {
  return { key: `${from}-${at}`, from, text, at };
}

describe("buildChatTranscript", () => {
  it("names every sender role, a subagent bubble included", () => {
    // Given
    const messages = [aMessage("user", "Add a health check.", 1_000), aMessage("subagent", "reviewer", 2_000)];

    // When
    const transcript = buildChatTranscript(messages, []);

    // Then
    expect(transcript).toContain("You: Add a health check.");
    expect(transcript).toContain("Subagent: reviewer");
    expect(transcript).not.toContain("undefined");
  });
});

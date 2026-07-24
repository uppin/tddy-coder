import type { ChatMessage } from "./useAgentChat";

/**
 * The streaming agent-output reconciliation used to turn raw `AgentMessageChunk` text into chat
 * bubbles — the same merge tddy-core's `AgentOutputActivityLogMerge` applies (and that
 * `useAcpSession` / `useAgentChat` inline for the live path). Deltas accumulate into ONE growing
 * agent bubble; a `\n` finalizes the current line; a re-emitted snapshot identical to the line
 * already shown is dropped. It mutates the caller's `messages` array in place (touching only a
 * trailing `from: "agent"` bubble) so it composes with non-agent bubbles the caller pushes between
 * chunks.
 *
 * `createAgentChunkMerger` returns a stateful merger with its own delta buffer, partial-active flag,
 * and key counter, so one instance owns one logical stream. `finalize` flushes any pending partial
 * as a finalized line and clears the partial state — replay callers call it after each discrete
 * recorded chunk so consecutive recorded chunks render as separate bubbles rather than concatenating
 * into one.
 */
export function createAgentChunkMerger() {
  let buffer = "";
  let partialActive = false;
  let keyCounter = 0;

  const lastAgentIndex = (messages: ChatMessage[]): number =>
    messages.length > 0 && messages[messages.length - 1].from === "agent" ? messages.length - 1 : -1;

  const pushAgentLine = (messages: ChatMessage[], text: string, at: number) => {
    messages.push({ key: `agent-line-${keyCounter++}`, text, from: "agent", at });
  };

  /** A finalized line (delta buffer flushed on `\n`): replace the in-progress partial row, else
   *  append — but drop a re-emitted snapshot identical to the line already shown. */
  const finalizeAgentLine = (messages: ChatMessage[], line: string, at: number) => {
    if (line.length === 0) return;
    if (partialActive) {
      const i = lastAgentIndex(messages);
      if (i >= 0) {
        messages[i] = { ...messages[i], text: line };
        partialActive = false;
        return;
      }
      partialActive = false;
    }
    const i = lastAgentIndex(messages);
    if (i >= 0 && messages[i].text === line) return;
    pushAgentLine(messages, line, at);
  };

  /** The still-growing (no newline yet) buffer: keep updating the one partial agent row. */
  const syncAgentPartial = (messages: ChatMessage[], buf: string, at: number) => {
    if (buf.length === 0) return;
    if (partialActive) {
      const i = lastAgentIndex(messages);
      if (i >= 0) {
        messages[i] = { ...messages[i], text: buf };
        return;
      }
    }
    pushAgentLine(messages, buf, at);
    partialActive = true;
  };

  return {
    /** Apply one raw `AgentMessageChunk` chunk the way `AgentOutputActivityLogMerge::apply_chunk`
     *  does. New bubbles are stamped with `at`; updates to an existing bubble keep its original `at`. */
    appendChunk(messages: ChatMessage[], text: string, at: number) {
      // split_inclusive('\n'): each part keeps its trailing newline (if any).
      const parts = text.match(/[^\n]*\n|[^\n]+/g) ?? [];
      for (const part of parts) {
        if (part.endsWith("\n")) {
          buffer += part.slice(0, -1);
          const line = buffer;
          buffer = "";
          finalizeAgentLine(messages, line, at);
        } else {
          buffer += part;
        }
      }
      syncAgentPartial(messages, buffer, at);
    },

    /** Flush any pending partial buffer as a finalized line and end the current line, so the next
     *  `appendChunk` starts a fresh bubble. */
    finalize(messages: ChatMessage[], at: number) {
      if (buffer.length > 0) {
        const line = buffer;
        buffer = "";
        finalizeAgentLine(messages, line, at);
      }
      partialActive = false;
    },
  };
}

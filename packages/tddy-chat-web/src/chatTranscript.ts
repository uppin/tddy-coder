import type { ChatMessage, ElicitationPoint } from "./useAgentChat";

/** ISO-8601 timestamp (UTC) — unambiguous for reading a session timeline. */
function ts(at: number): string {
  return new Date(at).toISOString();
}

const ROLE: Record<ChatMessage["from"], string> = {
  user: "You",
  agent: "Agent",
  goal: "Goal",
  activity: "Activity",
  tool: "Tool",
};

/**
 * Build a plain-text transcript with timestamps, merging chat messages and elicitation points into
 * one chronological timeline so it's clear when the agent paused for a clarification and what was
 * asked/answered.
 */
export function buildChatTranscript(
  messages: ChatMessage[],
  elicitations: ElicitationPoint[],
): string {
  const rows: { at: number; line: string }[] = [
    ...messages.map((m) => ({ at: m.at, line: `[${ts(m.at)}] ${ROLE[m.from]}: ${m.text}` })),
    ...elicitations.map((e) => ({
      at: e.at,
      line:
        `[${ts(e.at)}] ❓ CLARIFICATION (${e.kind}${e.allowOther ? ", allows other" : ""})` +
        `${e.header ? ` [${e.header}]` : ""}: ${e.question}` +
        (e.options.length ? `\n      options: ${e.options.join(" | ")}` : ""),
    })),
  ];
  rows.sort((a, b) => a.at - b.at);
  const header = `TDDY chat transcript — exported ${new Date().toISOString()}\n${"=".repeat(64)}\n`;
  return header + rows.map((r) => r.line).join("\n") + "\n";
}

/** Trigger a browser download of `content` as a text file named `filename`. */
export function downloadTextFile(filename: string, content: string): void {
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

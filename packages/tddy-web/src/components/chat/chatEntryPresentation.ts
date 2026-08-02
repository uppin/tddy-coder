/**
 * Presentation of a single transcript entry, shared by the live chat and the read-only transcript.
 *
 * The two surfaces render different chrome around an entry — one has a composer, the other elapsed
 * badges and status markers — but the entry itself has to look the same in both, so the classes it
 * is built from live here rather than in either surface.
 */

import type { ChatMessage } from "./useAgentChat";

/** Tailwind classes for a chat bubble by role. Shared by the live and read-only renders so their
 *  common roles look identical. */
export function bubbleClass(from: ChatMessage["from"]): string {
  switch (from) {
    case "user":
      return "self-end rounded-md bg-primary text-primary-foreground px-3 py-2 text-sm";
    case "agent":
      return "self-start rounded-md bg-muted px-3 py-2 text-sm";
    case "goal":
      return "self-center text-xs text-muted-foreground italic font-medium";
    case "tool":
      return "self-start rounded-md border border-border bg-muted/50 px-3 py-2 font-mono text-xs";
    default:
      return "self-center text-xs text-muted-foreground italic";
  }
}

/** Classes for the tool-call status marker on a read-only transcript entry. */
export function toolStatusClass(status: NonNullable<ChatMessage["toolStatus"]>): string {
  const base = "rounded px-1.5 py-0.5 text-[10px] font-medium leading-none";
  if (status === "error") return `${base} text-destructive`;
  if (status === "completed") return `${base} text-muted-foreground`;
  return `${base} text-primary`; // running
}

/** DEBUG-style "+Ns" (or "+Nms" under a second) elapsed badge: the gap from the previous entry.
 *  The first entry has no predecessor, so it reads "+0ms". */
export function elapsedBadge(messages: ChatMessage[], index: number): string {
  const ms = index === 0 ? 0 : messages[index].at - messages[index - 1].at;
  return ms >= 1000 ? `+${Math.round(ms / 1000)}s` : `+${ms}ms`;
}

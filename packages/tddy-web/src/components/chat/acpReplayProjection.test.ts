/**
 * Unit tests — projecting `StreamAcpReplay` frames into transcript entries.
 *
 * The projection used to live inside `useAcpReplay`'s stream effect, one frame at a time. Paging
 * needs it as a function: a whole page arrives at once from `GetAcpReplayPage` and has to become
 * entries before it can be prepended. Extracting it also exposes the constraint the inline version
 * never had to meet — entries from two different pages share one rendered list, so their keys must
 * not collide.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Tail-first, auto-scrolling transcript.
 */

import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
  AcpAgentMessageSchema,
  ToolCallStatus,
  ToolKind,
  type AcpAgentMessage,
} from "../../gen/tddy/acp/v1/acp_pb";
import { createReplayProjector, projectReplayFrames } from "./acpReplayProjection";

/** A recorded agent text chunk — projects to an "agent" bubble. */
function anAgentTextFrame(text: string, atUnixMs: number): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        timestampUnixMs: BigInt(atUnixMs),
        update: {
          update: {
            case: "agentMessageChunk",
            value: { content: { block: { case: "text", value: { text } } } },
          },
        },
      },
    },
  });
}

/** A recorded tool call, metadata only — the hosts strip the bodies out of every streamed frame. */
function aToolCallFrame(fields: {
  id: string;
  title: string;
  status: ToolCallStatus;
  atUnixMs: number;
}): AcpAgentMessage {
  return create(AcpAgentMessageSchema, {
    id: 0n,
    msg: {
      case: "sessionUpdate",
      value: {
        sessionId: { value: "s1" },
        timestampUnixMs: BigInt(fields.atUnixMs),
        update: {
          update: {
            case: "toolCall",
            value: {
              toolCallId: { value: fields.id },
              title: fields.title,
              kind: ToolKind.EXECUTE,
              status: fields.status,
            },
          },
        },
      },
    },
  });
}

/** One page's worth of frames — the same shape twice over, so a key collision between two pages
 *  shows up as a collision rather than being hidden by differing content. */
function aPageOfFrames(): AcpAgentMessage[] {
  return [
    anAgentTextFrame("Reading the parser.", 1_000),
    aToolCallFrame({
      id: "tool-1",
      title: "Read main.rs",
      status: ToolCallStatus.COMPLETED,
      atUnixMs: 2_000,
    }),
  ];
}

describe("projectReplayFrames", () => {
  it("projects a page of frames into transcript entries in recorded order", () => {
    // Given — an interleaved page: agent text, a tool call, agent text
    const frames = [
      anAgentTextFrame("Let me read the file.", 1_000),
      aToolCallFrame({
        id: "tool-1",
        title: "Read main.rs",
        status: ToolCallStatus.COMPLETED,
        atUnixMs: 2_000,
      }),
      anAgentTextFrame("Now I understand it.", 3_000),
    ];

    // When — the whole page is projected at once
    const entries = projectReplayFrames(frames, 0);

    // Then — three entries in the recorded order, each carrying its recorded timestamp
    expect(entries.map((entry) => [entry.from, entry.text, entry.at])).toEqual([
      ["agent", "Let me read the file.", 1_000],
      ["tool", "Read main.rs", 2_000],
      ["agent", "Now I understand it.", 3_000],
    ]);
  });

  it("coalesces a tool call's running then completed frames within one page", () => {
    // Given — one call recorded twice under the same id as it progressed
    const frames = [
      aToolCallFrame({
        id: "tool-1",
        title: "Bash cargo test",
        status: ToolCallStatus.IN_PROGRESS,
        atUnixMs: 1_000,
      }),
      aToolCallFrame({
        id: "tool-1",
        title: "Bash cargo test",
        status: ToolCallStatus.COMPLETED,
        atUnixMs: 3_000,
      }),
    ];

    // When
    const entries = projectReplayFrames(frames, 0);

    // Then — one entry in its terminal state, not two rows for one call
    expect(entries.map((entry) => [entry.from, entry.toolStatus, entry.at])).toEqual([
      ["tool", "completed", 3_000],
    ]);
  });

  it("gives two pages disjoint entry keys", () => {
    // Given — the newest page and the older page behind it, projected from identical frame shapes
    const tailPage = projectReplayFrames(aPageOfFrames(), 150);
    const olderPage = projectReplayFrames(aPageOfFrames(), 50);

    // When — both pages end up in the one rendered list a prepend produces
    const keys = new Set([...olderPage, ...tailPage].map((entry) => entry.key));

    // Then — every entry keeps its own key. React reconciles the prepended page against the loaded
    // range by key, so a collision silently drops or reuses a rendered row.
    expect(keys.size).toBe(olderPage.length + tailPage.length);
  });
});

describe("createReplayProjector", () => {
  it("accumulates the live tail onto the entries the page already produced", () => {
    // Given — a projector opened over the tail page's position
    const projector = createReplayProjector(150);

    // When — frames arrive one at a time, as the live tail delivers them
    projector.append(anAgentTextFrame("First.", 1_000));
    const entries = projector.append(anAgentTextFrame("Second.", 2_000));

    // Then — the running list carries both, in arrival order: a live frame extends the transcript
    // rather than replacing it
    expect(entries.map((entry) => entry.text)).toEqual(["First.", "Second."]);
  });
});

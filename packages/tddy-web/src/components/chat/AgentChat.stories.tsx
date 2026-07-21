import type { Meta, StoryObj } from "@storybook/react";
import { AgentChatView } from "./AgentChat";
import type { ChatMessage, PendingQuestion, UseAgentChatResult } from "./useAgentChat";

/**
 * Stories for the recipe-agnostic Agent Chat panel. They render the pure presentational
 * `AgentChatView` with a hand-built `chat` result — no LiveKit room or RPC backend — so each of the
 * states the live hook (`useAgentChat` / `useAcpSession`) can produce is visible in isolation:
 * empty, streaming, a select clarification, a multi-select clarification, an error, and connecting.
 */

const now = Date.now();

/** Build a `UseAgentChatResult` for a story; actions are inert no-ops (return `true`). */
function chat(overrides: Partial<UseAgentChatResult> = {}): UseAgentChatResult {
  return {
    messages: [],
    elicitations: [],
    sendPrompt: () => true,
    pendingQuestion: null,
    answerSelect: () => true,
    answerOther: () => true,
    answerMultiSelect: () => true,
    streamError: null,
    sendError: null,
    workflowError: null,
    ...overrides,
  };
}

const streamingMessages: ChatMessage[] = [
  { key: "g0", from: "goal", text: "analyze-stack", at: now },
  { key: "u0", from: "user", text: "Split this feature into a PR stack.", at: now + 1_000 },
  { key: "a0", from: "agent", text: "Analyzing the feature into a PR stack…", at: now + 2_000 },
  { key: "act0", from: "activity", text: "Reading packages/tddy-core/src/lib.rs", at: now + 2_500 },
];

const selectQuestion: PendingQuestion = {
  kind: "select",
  header: "Backend",
  question: "Which coding backend should drive this session?",
  options: [
    { label: "Claude", description: "Anthropic Claude via the ACP agent" },
    { label: "Cursor", description: "Cursor Agent CLI" },
  ],
  allowOther: true,
};

const multiSelectQuestion: PendingQuestion = {
  kind: "multiSelect",
  header: "Steps",
  question: "Which workflow steps should run?",
  options: [
    { label: "Plan", description: "Produce PRD.md + TODO.md" },
    { label: "Red", description: "Write failing tests" },
    { label: "Green", description: "Implement to green" },
  ],
  allowOther: true,
};

const meta: Meta<typeof AgentChatView> = {
  component: AgentChatView,
  // Give the flex-1 panel a bounded, chat-sized frame so it lays out like the session drawer.
  decorators: [
    (Story) => (
      <div style={{ height: 520, width: 420, display: "flex", flexDirection: "column" }}>
        <Story />
      </div>
    ),
  ],
  args: {
    room: null,
    placeholder: "Message the agent…",
    roomStatus: "connected",
  },
};

export default meta;

type Story = StoryObj<typeof AgentChatView>;

export const Empty: Story = {
  args: { chat: chat() },
};

export const Streaming: Story = {
  args: { chat: chat({ messages: streamingMessages }) },
};

export const SelectQuestion: Story = {
  args: {
    chat: chat({ messages: streamingMessages, pendingQuestion: selectQuestion }),
  },
};

export const MultiSelectQuestion: Story = {
  args: {
    chat: chat({ messages: streamingMessages, pendingQuestion: multiSelectQuestion }),
  },
};

export const Error: Story = {
  args: {
    chat: chat({ messages: streamingMessages, streamError: "presenter participant left the room" }),
  },
};

export const Connecting: Story = {
  args: {
    roomStatus: "connecting",
    chat: chat(),
  },
};

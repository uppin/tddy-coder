import type { Meta, StoryObj } from "@storybook/react";
import { create } from "@bufbuild/protobuf";
import { SessionEntrySchema } from "tddy-client-web/gen/session_pb";
import { SessionNameEditor } from "tddy-session-web";

const A_SESSION = {
  sessionId: "story-session-1",
  createdAt: "2026-11-15T10:00:00Z",
  status: "active",
  isActive: true,
  pid: 4242,
  repoPath: "/home/dev/my-feature-branch",
};

const meta: Meta<typeof SessionNameEditor> = {
  title: "tddy-session-web/SessionNameEditor",
  component: SessionNameEditor,
  decorators: [
    (Story) => (
      <div style={{ width: 340, padding: 12 }}>
        <Story />
      </div>
    ),
  ],
};

export default meta;

type Story = StoryObj<typeof SessionNameEditor>;

/** No name yet: the derived label (the worktree's basename) is shown, muted. */
export const Unnamed: Story = {
  args: {
    session: create(SessionEntrySchema, { ...A_SESSION, displayName: "" }),
    onRename: async () => undefined,
  },
};

export const Named: Story = {
  args: {
    session: create(SessionEntrySchema, { ...A_SESSION, displayName: "Fix the login flow" }),
    onRename: async () => undefined,
  },
};

/** The host refuses every name: Rename, type, Enter shows the refusal beside the editor. */
export const Refused: Story = {
  args: {
    session: create(SessionEntrySchema, { ...A_SESSION, displayName: "Fix the login flow" }),
    onRename: async () => {
      throw new Error("display name is longer than 80 characters");
    },
  },
};

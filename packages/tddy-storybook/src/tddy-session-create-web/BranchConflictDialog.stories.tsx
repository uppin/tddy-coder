import type { Meta, StoryObj } from "@storybook/react";
import { create } from "@bufbuild/protobuf";
import { BranchConflictSchema } from "tddy-client-web/gen/session_pb";
import { BranchSessionSchema } from "tddy-client-web/gen/types_pb";
import { BranchConflictDialog } from "tddy-session-create-web";

const conflict = create(BranchConflictSchema, {
  branch: "feat/auth",
  owner: create(BranchSessionSchema, { exists: true, sessionId: "019a-owner", isActive: true, status: "running" }),
  suggestedBranchName: "feat/auth-1",
});

const meta: Meta<typeof BranchConflictDialog> = {
  title: "tddy-session-create-web/BranchConflictDialog",
  component: BranchConflictDialog,
  args: {
    conflict,
    onSwitchToOwner: () => {},
    onAddAgent: () => {},
    onRename: () => {},
    onCancel: () => {},
  },
};

export default meta;

type Story = StoryObj<typeof BranchConflictDialog>;

export const OwnedBranch: Story = {};

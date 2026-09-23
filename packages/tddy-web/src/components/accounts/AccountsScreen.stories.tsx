import type { Meta, StoryObj } from "@storybook/react";
import { AccountsScreen, type AccountRow } from "./AccountsScreen";

const meta: Meta<typeof AccountsScreen> = {
  component: AccountsScreen,
  args: {
    onRename: () => {},
    onRemove: () => {},
  },
};

export default meta;

type Story = StoryObj<typeof AccountsScreen>;

function anAccount(accountId: string, label: string, subject: string): AccountRow {
  return { accountId, label, subject, updatedAtUnixSeconds: 1_726_700_000n, hasSecret: true };
}

export const Listed: Story = {
  args: {
    outcome: {
      kind: "listed",
      providers: [
        { provider: "cloudflare", accounts: [anAccount("zoe", "Zone admin", "zoe@example.com")] },
        {
          provider: "github",
          accounts: [
            anAccount("ada", "Ada at work", "ada-lovelace"),
            { ...anAccount("bob", "Bob the bot", "bob-bot"), hasSecret: false },
          ],
        },
      ],
    },
  },
};

export const Empty: Story = {
  args: { outcome: { kind: "listed", providers: [] } },
};

export const Locked: Story = {
  args: { outcome: { kind: "locked" } },
};

export const Errored: Story = {
  args: { outcome: { kind: "error", reason: "the vault file is truncated" } },
};

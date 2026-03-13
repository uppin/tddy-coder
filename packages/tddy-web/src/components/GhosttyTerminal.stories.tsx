import type { Meta, StoryObj } from "@storybook/react";
import { GhosttyTerminal } from "./GhosttyTerminal";
import { GhosttyTerminalLiveKit } from "./GhosttyTerminalLiveKit";

const meta: Meta<typeof GhosttyTerminal> = {
  component: GhosttyTerminal,
};

export default meta;

type Story = StoryObj<typeof GhosttyTerminal>;

export const Default: Story = {
  args: {
    onData: () => {},
    onResize: () => {},
    onBell: () => {},
    onTitleChange: () => {},
  },
};

export const WithContent: Story = {
  args: {
    initialContent:
      "\x1b[1;32m$ ls -la\x1b[0m\nfile1.txt  file2.txt  \x1b[1;34mdir\x1b[0m/\n\x1b[1;32m$ echo hello\x1b[0m\nhello",
    onData: () => {},
    onResize: () => {},
    onBell: () => {},
    onTitleChange: () => {},
  },
};

const colorPaletteContent = [
  "\x1b[30mBlack\x1b[0m \x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m \x1b[33mYellow\x1b[0m",
  "\x1b[34mBlue\x1b[0m \x1b[35mMagenta\x1b[0m \x1b[36mCyan\x1b[0m \x1b[37mWhite\x1b[0m",
  "\x1b[90mBrightBlack\x1b[0m \x1b[91mBrightRed\x1b[0m \x1b[92mBrightGreen\x1b[0m \x1b[93mBrightYellow\x1b[0m",
  "\x1b[94mBrightBlue\x1b[0m \x1b[95mBrightMagenta\x1b[0m \x1b[96mBrightCyan\x1b[0m \x1b[97mBrightWhite\x1b[0m",
].join("\n");

export const ColorPalette: Story = {
  args: {
    initialContent: colorPaletteContent,
    onData: () => {},
    onResize: () => {},
    onBell: () => {},
    onTitleChange: () => {},
  },
};

function LiveKitConnectedStory(args: {
  url?: string;
  token?: string;
  roomName?: string;
  showBufferTextForTest?: boolean;
  debugMode?: boolean;
}) {
  const params = typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
  const url = params?.get("url") ?? args.url ?? "";
  const token = params?.get("token") ?? args.token ?? "";
  const roomName = params?.get("roomName") ?? args.roomName ?? "terminal-e2e";
  const showBufferTextForTest = args.showBufferTextForTest ?? true;
  const debugMode = args.debugMode ?? params?.get("debugMode") === "1";

  if (!url || !token) {
    return (
      <div data-testid="livekit-placeholder">
        Provide url and token via Storybook args or URL params (?url=...&amp;token=...&amp;roomName=...)
      </div>
    );
  }

  return (
    <GhosttyTerminalLiveKit
      url={url}
      token={token}
      roomName={roomName}
      showBufferTextForTest={showBufferTextForTest}
      debugMode={debugMode}
    />
  );
}

export const LiveKitConnected: StoryObj<typeof GhosttyTerminalLiveKit> = {
  render: (args) => <LiveKitConnectedStory {...args} />,
  args: {
    url: "",
    token: "",
    roomName: "terminal-e2e",
    showBufferTextForTest: true,
  },
  argTypes: {
    url: { control: "text" },
    token: { control: "text" },
    roomName: { control: "text" },
  },
};

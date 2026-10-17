#!/usr/bin/env bun
/**
 * Run the wheel-reporting test TUI in a real terminal — e.g. inside a tddy session — to see which
 * input the terminal actually delivers to a full-screen application.
 *
 *   bun packages/tddy-web/scripts/wheel-tui/wheel-tui.ts [--normal-screen] [--mouse=1000|1002|1003|off]
 *
 * Defaults: alternate screen, mouse mode 1002. Quit with `q` or Ctrl+C.
 */
import { WheelTuiApp, type MouseTrackingMode } from "./wheelTuiApp";

const args = process.argv.slice(2);
const mouseArg = args.find((a) => a.startsWith("--mouse="))?.slice("--mouse=".length) ?? "1002";
if (!["1000", "1002", "1003", "off"].includes(mouseArg)) {
  process.stderr.write(`unknown --mouse=${mouseArg} (expected 1000, 1002, 1003 or off)\n`);
  process.exit(2);
}

const app = new WheelTuiApp({
  alternateScreen: !args.includes("--normal-screen"),
  mouse: mouseArg as MouseTrackingMode,
});

process.stdin.setRawMode(true);
process.stdin.resume();
process.stdout.write(app.start());

const quit = () => {
  process.stdout.write(app.stop());
  process.stdin.setRawMode(false);
  process.exit(0);
};
process.stdin.on("data", (chunk: Buffer) => {
  const input = chunk.toString("utf8");
  if (input === "q" || input === "\x03") quit();
  else process.stdout.write(app.receive(input));
});
process.on("SIGTERM", quit);

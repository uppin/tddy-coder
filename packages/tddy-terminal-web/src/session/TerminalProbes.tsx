import type { TerminalMirrorValues } from "./useTerminalMirrors";

/** Hidden elements through which component tests observe the terminal's state. */
export function TerminalProbes({
  mirrors,
  firstOutputReceived,
  streamedByteCount,
  showBufferTextForTest,
}: {
  mirrors: TerminalMirrorValues;
  firstOutputReceived: boolean;
  streamedByteCount: number;
  showBufferTextForTest: boolean;
}) {
  const {
    bufferText,
    olderBufferText,
    liveScrollbackLength,
    pageScrollbar,
    liveScrollbar,
    highlightedLine,
  } = mirrors;
  return (
    <>
        {firstOutputReceived && (
          <div data-testid="first-output-received" style={{ display: "none" }} aria-hidden />
        )}
        <div data-testid="terminal-buffer-text" style={{ display: "none" }} aria-hidden>
          {bufferText}
        </div>
        <div data-testid="terminal-older-buffer-text" style={{ display: "none" }} aria-hidden>
          {olderBufferText}
        </div>
        <div data-testid="terminal-live-scrollback-length" style={{ display: "none" }} aria-hidden>
          {liveScrollbackLength}
        </div>
        <div data-testid="terminal-page-scrollbar" style={{ display: "none" }} aria-hidden>
          {pageScrollbar}
        </div>
        <div data-testid="terminal-live-scrollbar" style={{ display: "none" }} aria-hidden>
          {liveScrollbar}
        </div>
        {showBufferTextForTest && (
          <>
            <div data-testid="streamed-byte-count" style={{ display: "none" }} aria-hidden>
              {streamedByteCount}
            </div>
            <div data-testid="terminal-highlighted-line" style={{ display: "none" }} aria-hidden>
              {highlightedLine}
            </div>
          </>
        )}
    </>
  );
}

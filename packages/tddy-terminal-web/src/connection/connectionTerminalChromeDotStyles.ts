/** Shared keyframes + dot sizing for all ConnectionTerminalChrome layouts. */
export const CONNECTION_TERMINAL_DOT_STYLES = `
@keyframes tddy-dot-pulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.55; transform: scale(0.92); }
}
.tddy-connection-dot-inner {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  display: inline-block;
  vertical-align: middle;
}
.tddy-connection-dot-inner--compact {
  width: 10px;
  height: 10px;
}
.tddy-connection-dot--connecting {
  background: #f59e0b;
  animation: tddy-dot-pulse 1.2s ease-in-out infinite;
}
.tddy-connection-dot--connected {
  background: #22c55e;
}
.tddy-connection-dot--error {
  background: #ef4444;
}
@media (prefers-reduced-motion: reduce) {
  .tddy-connection-dot--connecting {
    animation: none;
    opacity: 0.85;
  }
}
`;

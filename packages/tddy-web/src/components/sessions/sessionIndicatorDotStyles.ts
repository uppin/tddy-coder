/**
 * Keyframes for the drawer's `working` dot — the green one that fades in and out while the agent is
 * busy (PRD: docs/ft/daemon/session-notifications.md, FR4/NFR2).
 *
 * Injected as an inline `<style>` next to the dots, following the one precedent the app already has
 * for a hand-written animation, `connection/connectionTerminalChromeDotStyles.ts`.
 *
 * Opacity only, deliberately: the dot sits in a flex row beside the row's label, and animating
 * `transform: scale()` there makes the neighbouring text look like it is breathing. Fading is the
 * quieter signal for something a drawer may show on a dozen rows at once.
 */
export const SESSION_INDICATOR_DOT_STYLES = `
@keyframes tddy-session-dot-blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.25; }
}
.tddy-session-dot--working {
  animation: tddy-session-dot-blink 1.2s ease-in-out infinite;
}
@media (prefers-reduced-motion: reduce) {
  .tddy-session-dot--working {
    animation: none;
    opacity: 1;
  }
}
`;

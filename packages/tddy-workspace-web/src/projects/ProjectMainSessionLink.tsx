import { sessionsDrawerPathForSession, sessionsProjectHomeSetupPath } from "tddy-client-web";

/**
 * A Projects card's link to the project's **main** session: **Set up main** opens the sessions
 * drawer's setup pane for the project when it has none, **Open main** opens main itself.
 *
 * `#orchestrators` 11/11: **Set up main** opens the project's settings pane on the host whose row
 * the card read (`#/sessions/project/<id>/settings?host=<daemonInstanceId>`), so the card and the
 * pane cannot disagree about whose main they mean.
 *
 * PRD: docs/ft/web/session-drawer.md
 */
export function ProjectMainSessionLink({
  projectId,
  homeSessionId,
  daemonInstanceId,
}: {
  projectId: string;
  /** The session the project records as its main; empty when main is not set up. */
  homeSessionId: string;
  /**
   * The host whose row `homeSessionId` was read from — the card's primary host.
   * TODO(project-settings-pane): implement — not read yet; the link still names no host.
   */
  daemonInstanceId: string;
}) {
  void daemonInstanceId;
  const hasMain = homeSessionId !== "";
  const path = hasMain ? sessionsDrawerPathForSession(homeSessionId) : sessionsProjectHomeSetupPath(projectId);
  return (
    <div className="mb-3 text-sm">
      <a
        data-testid={`project-main-session-link-${projectId}`}
        href={`#${path}`}
        className="text-primary underline-offset-4 hover:underline"
      >
        {hasMain ? "Open main" : "Set up main"}
      </a>
    </div>
  );
}

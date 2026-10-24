import { sessionsDrawerPathForSession, sessionsProjectHomeSetupPath } from "../../routing/appRoutes";

/**
 * A Projects card's link to the project's **main** session: **Set up main** opens the sessions
 * drawer's setup pane for the project when it has none, **Open main** opens main itself.
 *
 * PRD: docs/ft/web/session-drawer.md
 */
export function ProjectMainSessionLink({
  projectId,
  homeSessionId,
}: {
  projectId: string;
  /** The session the project records as its main; empty when main is not set up. */
  homeSessionId: string;
}) {
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

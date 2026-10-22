import type React from "react";
import type { SessionEntry } from "../../gen/session_pb";
import type { ProjectHomeInfo } from "./SessionDrawer";

/**
 * The pinned **main** entry first in a project group (`#project-home` 4/7): label `main`, the
 * project's main branch ref, the home agent's label, and the home session's status dot once it
 * exists. Clicking it ensures the project-home session; a refusal is shown on the entry. With no
 * home agent it shows a hint pointing at the Projects screen and starts nothing.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-10-22-project-home-session.md.
 */
export function ProjectHomeEntry(props: {
  home: ProjectHomeInfo;
  homeSession: SessionEntry | null;
  selected: boolean;
  onEnsure: (projectId: string) => Promise<void>;
}): React.ReactElement | null {
  // TODO(home-session): implement
  void props;
  return null;
}

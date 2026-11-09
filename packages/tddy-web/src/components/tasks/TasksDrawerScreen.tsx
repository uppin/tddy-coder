import { TasksScreen } from "tddy-workspace-web";
import { AppShell } from "../shell/AppShell";

/** The Tasks route: the application shell around the workspace package's tasks screen. */
export function TasksDrawerScreen({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <AppShell variant="fullbleed" title="Tasks" onNavigate={onNavigate}>
      <TasksScreen />
    </AppShell>
  );
}

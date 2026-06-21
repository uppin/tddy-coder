import { DaemonNavMenu } from "../shell/DaemonNavMenu";
import { UserAvatar } from "../UserAvatar";
import { VmsScreen } from "./VmsScreen";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

export function VmsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  return (
    <div className={screenShellClassName}>
      <div className="flex items-center gap-3 mb-6">
        <DaemonNavMenu onNavigate={onNavigate} />
        <h1 className="text-xl font-bold flex-1">VMs</h1>
        <UserAvatar />
      </div>
      {/* TODO: wire RPC client for ListVms/StartVm/StopVm/RemoveVm in /green phase */}
      <VmsScreen
        rows={[]}
        onStart={() => {}}
        onStop={() => {}}
        onRemove={() => {}}
      />
    </div>
  );
}

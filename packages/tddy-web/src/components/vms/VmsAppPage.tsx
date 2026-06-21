import { useCallback, useEffect, useMemo, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { VmService, type VmInfo } from "../../gen/vm_pb";
import { useAuth } from "../../hooks/useAuth";
import { DaemonNavMenu } from "../shell/DaemonNavMenu";
import { UserAvatar } from "../UserAvatar";
import { VmsScreen, type VmRow } from "./VmsScreen";
import { DefineVmPanel } from "./DefineVmPanel";

const screenShellClassName =
  "min-h-svh w-full min-w-0 box-border px-4 py-6 sm:px-6 font-sans text-foreground";

function createVmClient() {
  const transport = createConnectTransport({
    baseUrl: typeof window !== "undefined" ? `${window.location.origin}/rpc` : "",
    useBinaryFormat: true,
  });
  return createClient(VmService, transport);
}

function vmStateLabel(state: number): string {
  switch (state) {
    case 1: return "Defined";
    case 2: return "Booting";
    case 3: return "Running";
    case 4: return "Stopped";
    case 5: return "Error";
    default: return "Unknown";
  }
}

function rowFromRpc(vm: VmInfo): VmRow {
  return {
    name: vm.name,
    state: vmStateLabel(vm.state),
    sshHostPort: vm.sshHostPort,
    shareUrl: vm.shareUrl,
    errorMessage: vm.errorMessage,
  };
}

export function VmsAppPage({ onNavigate }: { onNavigate: (path: string) => void }) {
  const { user, logout, sessionToken } = useAuth();
  const client = useMemo(() => createVmClient(), []);

  const [rows, setRows] = useState<VmRow[]>([]);
  const [building, setBuilding] = useState(false);
  const [availableImages, setAvailableImages] = useState<string[]>([]);
  const [buildError, setBuildError] = useState("");

  const loadVms = useCallback(() => {
    if (!sessionToken) return;
    client
      .listVms({ sessionToken })
      .then((res) => setRows(res.vms.map(rowFromRpc)))
      .catch(() => {});
  }, [client, sessionToken]);

  useEffect(() => {
    loadVms();
  }, [loadVms]);

  const handleBuild = useCallback(
    (spec: string) => {
      if (!sessionToken) return;
      setBuilding(true);
      setBuildError("");
      client
        .buildVmImage({ sessionToken, buildTarget: spec })
        .then((res) => {
          if (res.ok) {
            setAvailableImages((prev) =>
              prev.includes(res.imagePath) ? prev : [...prev, res.imagePath]
            );
          } else {
            setBuildError(res.message || "Build failed");
          }
        })
        .catch((e: unknown) => {
          setBuildError(e instanceof Error ? e.message : "Build failed");
        })
        .finally(() => setBuilding(false));
    },
    [client, sessionToken]
  );

  const handleDefineVm = useCallback(
    (name: string, imagePath: string) => {
      if (!sessionToken) return;
      client
        .defineVm({
          sessionToken,
          spec: {
            name,
            imagePath,
            buildTarget: "",
            portForwards: [],
            sshHostPort: 0,
          },
        })
        .then(() => loadVms())
        .catch(() => {});
    },
    [client, sessionToken, loadVms]
  );

  const handleStart = useCallback(
    (name: string) => {
      if (!sessionToken) return;
      client
        .startVm({ sessionToken, name })
        .then(() => loadVms())
        .catch(() => {});
    },
    [client, sessionToken, loadVms]
  );

  const handleStop = useCallback(
    (name: string) => {
      if (!sessionToken) return;
      client
        .stopVm({ sessionToken, name })
        .then(() => loadVms())
        .catch(() => {});
    },
    [client, sessionToken, loadVms]
  );

  const handleRemove = useCallback(
    (name: string) => {
      if (!sessionToken) return;
      client
        .removeVm({ sessionToken, name })
        .then(() => loadVms())
        .catch(() => {});
    },
    [client, sessionToken, loadVms]
  );

  return (
    <div className={screenShellClassName}>
      <div className="flex items-center gap-3 mb-6">
        <DaemonNavMenu onNavigate={onNavigate} />
        <h1 className="text-xl font-bold flex-1">VMs</h1>
        {user ? <UserAvatar user={user} onLogout={logout} /> : null}
      </div>

      <div className="mb-8">
        <DefineVmPanel
          building={building}
          availableImages={availableImages}
          errorMessage={buildError}
          onBuild={handleBuild}
          onDefineVm={handleDefineVm}
        />
      </div>

      <VmsScreen
        rows={rows}
        onStart={handleStart}
        onStop={handleStop}
        onRemove={handleRemove}
      />
    </div>
  );
}

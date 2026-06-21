import { useCallback, useEffect, useMemo, useState } from "react";
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { VmService, type VmInfo, type VmImageInfo } from "../../gen/vm_pb";
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
  const [buildLog, setBuildLog] = useState<string[]>([]);

  const loadVms = useCallback(() => {
    if (!sessionToken) return;
    client
      .listVms({ sessionToken })
      .then((res) => setRows(res.vms.map(rowFromRpc)))
      .catch(() => {});
  }, [client, sessionToken]);

  const loadImages = useCallback(() => {
    if (!sessionToken) return;
    client
      .listVmImages({ sessionToken })
      .then((res) => {
        const paths = (res.images as VmImageInfo[]).map((img) => img.path);
        setAvailableImages(paths);
      })
      .catch(() => {});
  }, [client, sessionToken]);

  useEffect(() => {
    loadVms();
    loadImages();
  }, [loadVms, loadImages]);

  const handleBuild = useCallback(
    (spec: string) => {
      console.log("[VmsAppPage] handleBuild called, sessionToken=", sessionToken ? "present" : "missing", "spec length=", spec.length);
      if (!sessionToken) return;
      loadImages(); // refresh on start so any pre-existing images are visible
      setBuilding(true);
      setBuildError("");
      setBuildLog([]);

      (async () => {
        console.log("[VmsAppPage] buildVmImage dispatched, spec length=", spec.length);
        try {
          const stream = client.buildVmImage({ sessionToken, buildrootSpec: spec });
          for await (const progress of stream) {
            console.log("[VmsAppPage] progress stage=", progress.stage, "msg=", progress.message, "imagePath=", progress.imagePath);
            if (progress.message) {
              setBuildLog((prev) => [...prev, progress.message]);
            }
            // stage 4 = STAGE_DONE
            if (progress.stage === 4 && progress.imagePath) {
              // Immediate in-place merge for instant feedback during streaming
              setAvailableImages((prev) =>
                prev.includes(progress.imagePath) ? prev : [...prev, progress.imagePath]
              );
              // Full refresh from server to pick up correct metadata (size, mtime order)
              loadImages();
            }
            // stage 5 = STAGE_ERROR
            if (progress.stage === 5) {
              setBuildError(progress.message || "Build failed");
            }
          }
          console.log("[VmsAppPage] buildVmImage stream ended");
        } catch (e: unknown) {
          console.error("[VmsAppPage] buildVmImage error:", e);
          setBuildError(e instanceof Error ? e.message : "Build failed");
        } finally {
          setBuilding(false);
        }
      })();
    },
    [client, sessionToken, loadImages]
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
          buildLog={buildLog}
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

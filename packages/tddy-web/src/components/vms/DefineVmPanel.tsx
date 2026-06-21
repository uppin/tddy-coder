import { useState } from "react";
import { Button } from "@/components/ui/button";

export interface DefineVmPanelProps {
  building: boolean;
  builtImagePath: string;
  errorMessage: string;
  onBuildImage: (buildTarget: string) => void;
  onDefineVm: (name: string, imagePath: string) => void;
}

export function DefineVmPanel({
  building,
  builtImagePath,
  errorMessage,
  onBuildImage,
  onDefineVm,
}: DefineVmPanelProps) {
  const [buildTarget, setBuildTarget] = useState("");
  const [vmName, setVmName] = useState("");

  return (
    <div className="space-y-6">
      <section className="space-y-3">
        <h3 className="text-base font-semibold">Build disk image</h3>
        <div className="flex gap-2 items-end">
          <div className="flex-1">
            <label className="block text-sm mb-1 text-muted-foreground" htmlFor="define-vm-build-target-input">
              Build target
            </label>
            <input
              id="define-vm-build-target-input"
              data-testid="define-vm-build-target"
              type="text"
              className="w-full rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              placeholder="e.g. qemu-minimal:qcow2"
              value={buildTarget}
              onChange={(e) => setBuildTarget(e.target.value)}
              disabled={building}
            />
          </div>
          <Button
            type="button"
            data-testid="define-vm-build-btn"
            disabled={building || buildTarget.trim() === ""}
            onClick={() => onBuildImage(buildTarget.trim())}
          >
            Build image
          </Button>
        </div>
        {building && (
          <p data-testid="define-vm-building-status" className="text-sm text-muted-foreground animate-pulse">
            Building image…
          </p>
        )}
        {builtImagePath && (
          <p data-testid="define-vm-image-path" className="text-sm font-mono text-foreground break-all">
            {builtImagePath}
          </p>
        )}
        {errorMessage && (
          <p data-testid="define-vm-error" className="text-sm text-destructive">
            {errorMessage}
          </p>
        )}
      </section>

      <section className="space-y-3">
        <h3 className="text-base font-semibold">Create VM</h3>
        <div className="flex gap-2 items-end">
          <div className="flex-1">
            <label className="block text-sm mb-1 text-muted-foreground" htmlFor="define-vm-name-input">
              VM name
            </label>
            <input
              id="define-vm-name-input"
              data-testid="define-vm-name"
              type="text"
              className="w-full rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              placeholder="e.g. web-vm"
              value={vmName}
              onChange={(e) => setVmName(e.target.value)}
            />
          </div>
          <Button
            type="button"
            data-testid="define-vm-create-btn"
            disabled={vmName.trim() === "" || builtImagePath === ""}
            onClick={() => onDefineVm(vmName.trim(), builtImagePath)}
          >
            Create VM
          </Button>
        </div>
      </section>
    </div>
  );
}

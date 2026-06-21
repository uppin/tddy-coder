import { useState } from "react";
import { Button } from "@/components/ui/button";

export interface DefineVmPanelProps {
  building: boolean;
  availableImages: string[];
  errorMessage: string;
  onBuild: (spec: string) => void;
  onDefineVm: (name: string, imagePath: string) => void;
}

function basename(path: string): string {
  return path.split("/").pop() ?? path;
}

const inputClass =
  "w-full rounded-md border border-input bg-background px-3 py-1.5 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

export function DefineVmPanel({
  building,
  availableImages,
  errorMessage,
  onBuild,
  onDefineVm,
}: DefineVmPanelProps) {
  const [spec, setSpec] = useState("");
  const [selectedImage, setSelectedImage] = useState("");
  const [vmName, setVmName] = useState("");

  return (
    <div className="space-y-6">
      <section className="space-y-3">
        <h3 className="text-base font-semibold">Build disk image</h3>
        <div>
          <label
            className="block text-sm mb-1 text-muted-foreground"
            htmlFor="define-vm-spec-input"
          >
            Buildroot spec
          </label>
          <textarea
            id="define-vm-spec-input"
            data-testid="define-vm-spec"
            className={`${inputClass} font-mono resize-y`}
            rows={6}
            placeholder={"BR2_x86_64=y\nBR2_TOOLCHAIN_BUILDROOT_GLIBC=y\nBR2_TARGET_ROOTFS_EXT2=y"}
            value={spec}
            onChange={(e) => setSpec(e.target.value)}
            disabled={building}
          />
        </div>
        <div className="flex gap-2">
          <Button
            type="button"
            data-testid="define-vm-build-btn"
            disabled={building || spec.trim() === ""}
            onClick={() => onBuild(spec)}
          >
            Build image
          </Button>
        </div>
        {building && (
          <p
            data-testid="define-vm-building-status"
            className="text-sm text-muted-foreground animate-pulse"
          >
            Building image…
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
        <div>
          <label
            className="block text-sm mb-1 text-muted-foreground"
            htmlFor="define-vm-image-select-input"
          >
            Disk image
          </label>
          <select
            id="define-vm-image-select-input"
            data-testid="define-vm-image-select"
            className={inputClass}
            value={selectedImage}
            onChange={(e) => setSelectedImage(e.target.value)}
          >
            <option value="" disabled>
              {availableImages.length === 0
                ? "No images built yet"
                : "Select an image…"}
            </option>
            {availableImages.map((img) => (
              <option key={img} value={img}>
                {basename(img)}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label
            className="block text-sm mb-1 text-muted-foreground"
            htmlFor="define-vm-name-input"
          >
            VM name
          </label>
          <input
            id="define-vm-name-input"
            data-testid="define-vm-name"
            type="text"
            className={inputClass}
            placeholder="e.g. web-vm"
            value={vmName}
            onChange={(e) => setVmName(e.target.value)}
          />
        </div>
        <Button
          type="button"
          data-testid="define-vm-create-btn"
          disabled={vmName.trim() === "" || selectedImage === ""}
          onClick={() => onDefineVm(vmName.trim(), selectedImage)}
        >
          Create VM
        </Button>
      </section>
    </div>
  );
}

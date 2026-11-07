/**
 * Watches `window.devicePixelRatio` for changes — the signal ghostty-web 0.4.0
 * lacks. Its canvas renderer reads `devicePixelRatio` once at construction and
 * never again, so a terminal mounted on one display stays at that display's
 * scale factor forever: dragged onto a Retina screen it renders at 1x and the
 * compositor upscales it — soft, exactly-blurry text.
 *
 * A media query only fires while it matches, so the watch must re-arm at the
 * new resolution after every change (the pattern xterm's DevicePixelRatioMonitor
 * uses). Returns a dispose that stops watching.
 */
export function watchDevicePixelRatio(
  win: Window,
  onChange: (dpr: number) => void,
): () => void {
  // Very old webviews may not expose matchMedia; a terminal that can never see
  // a DPR change must still work.
  if (typeof win.matchMedia !== "function") {
    return () => {};
  }

  let media: MediaQueryList | null = null;
  let disposed = false;

  const handleChange = () => {
    if (disposed) return;
    onChange(win.devicePixelRatio);
    arm();
  };

  // Arm a fresh query at the current resolution: it fires as soon as the
  // devicePixelRatio moves away from it.
  const arm = () => {
    media?.removeEventListener("change", handleChange);
    media = win.matchMedia(`(resolution: ${win.devicePixelRatio}dppx)`);
    media.addEventListener("change", handleChange);
  };
  arm();

  return () => {
    disposed = true;
    media?.removeEventListener("change", handleChange);
    media = null;
  };
}

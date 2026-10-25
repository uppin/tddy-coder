# Files dropped on the window

How a file dragged from the OS onto the desktop window reaches a session terminal. The product-level
description is [web-terminal.md § In Tddy Desktop](../../../docs/ft/web/web-terminal.md#file-drop-in-tddy-desktop);
the page's half is [`tddy-web` terminal-file-upload.md](../../tddy-web/docs/terminal-file-upload.md#tddy-desktop-drops-arrive-from-the-window-not-the-dom).

| Package | Owns |
|---|---|
| `tddy-desktop/src-tauri` | `file_drop.rs`: the drop-scoped allowlist (`DroppedFiles`), the two read commands, the drag-step relay (`NativeDragStep`, `drag_step_of`); `lib.rs`: `relay_drags_to_the_page` on `on_window_event` |
| `tddy-tauri-web` | `nativeFileDrop.ts`: `NativeFileDropBridge` over the relay event and the commands |

## The native handler stays on

The window is built with Tauri's default `dragDropEnabled`. Its handler is the only thing that knows
a dropped file's **real path** — the webview's own HTML5 `drop` hands a page `File` objects with
none. The cost is that a file dragged from the OS is taken by that handler before WebKit sees it,
so the page receives no HTML5 `dragover` / `drop` for it; everything such a drop does is driven from
the window's own events.

## The relay: `tddy://native-file-drop`

`relay_drags_to_the_page` handles `WindowEvent::DragDrop` for the `main` window and emits each step,
labelled at that window, as `NATIVE_DRAG_EVENT` = `tddy://native-file-drop`:

```json
{ "type": "enter", "paths": ["/Users/me/a b.png"], "point": { "x": 412, "y": 300 } }
{ "type": "over",  "point": { "x": 413, "y": 301 } }
{ "type": "drop",  "paths": ["/Users/me/a b.png"], "point": { "x": 413, "y": 301 } }
{ "type": "leave" }
```

The page listens to this rather than to Tauri's own `tauri://drag-*` events because of the **unit of
the position**. Tauri types every drag position as a `PhysicalPosition`, but only WebView2 measures in
device pixels; WKWebView's `draggingLocation` and GTK's drag coordinates are already points — CSS
pixels. `drag_step_of` divides by the window's scale factor only where the platform reports device
pixels (`DRAG_POSITIONS_ARE_DEVICE_PIXELS` = `cfg!(windows)`), so `point` is always what
`document.elementFromPoint` takes. Dividing everywhere would put a drop on a Retina display at half
the pointer's coordinates. Paths travel exactly as the OS gave them; quoting is the page's job.

## Reading a dropped file: only the latest drop

A session the page cannot hand a real path to (another host, or a confined one) gets the file
uploaded, so the page needs its bytes. Two commands serve them, both declared in `build.rs` and
granted to the `main` window in `capabilities/default.json`:

| Command | Arguments | Returns |
|---|---|---|
| `tddy_dropped_files_describe` | `paths` | `[{ path, name, size, isFile }]`, in order — `size` 0 and `isFile` false for a folder |
| `tddy_dropped_file_read` | `path`, `offset`, `length` | the raw bytes (`tauri::ipc::Response`, not a JSON array): up to `length` from `offset`, fewer at the end, none past it |

Both answer **only for the paths of the most recent drop** onto the window. `DroppedFiles` holds that
set, replaced on every drop, and refuses any other path by name — these commands are not a general
file reader for whatever the page asks. A read is capped at `MAX_READ_BYTES` (1 MiB; the page reads
48 KiB upload chunks) and refused for anything that is not a regular file.

**Ordering.** The drop is recorded in `DroppedFiles` *before* its step is emitted, in the same
window-event callback, so by the time the page hears of a drop its files are readable.

## Tests

`file_drop.rs` unit tests: describe (order, names needing quoting, folders), refusal of an undropped
path, a new drop replacing the last, slice and tail reads, the read cap, a folder read; drag-step
mapping (paths unchanged, drop/leave, points not scaled where the platform reports points, divided
where it reports device pixels) and the step's wire shape.

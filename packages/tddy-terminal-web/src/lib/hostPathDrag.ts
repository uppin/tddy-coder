/**
 * The private drag MIME type that carries an already-uploaded file's absolute host path when a file
 * is dragged out of the Session Inspector → Files tab onto the terminal. `TerminalFileDropZone`
 * recognizes a drop carrying this type and inserts the quoted path rather than re-uploading (the
 * bytes are already on the host). An OS file drag carries `DataTransfer.files` instead.
 *
 * PRD: docs/ft/web/session-files-inspector.md
 */
export const HOST_PATH_MIME = "application/x-tddy-host-path";

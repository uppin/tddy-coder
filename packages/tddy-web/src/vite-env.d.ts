/// <reference types="vite/client" />

/** Set by `./web-dev` — browser-facing dev app origin (e.g. http://192.168.1.10:5173). */
interface ImportMetaEnv {
  readonly VITE_URL?: string;
  /** When `"true"`, emit extra terminal zoom bridge / sync logs. */
  readonly VITE_TERMINAL_ZOOM_DEBUG?: string;
}

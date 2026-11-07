/**
 * One Ghostty emulator instance per terminal, from a WASM module compiled once per page.
 *
 * ghostty-web's own `init()` shares a single instance across every terminal, and a terminal created
 * after another was freed is handed the freed one's memory unzeroed — a new session's pane opens on
 * the previous session's screen. A separate instance per terminal has memory of its own, released
 * with the terminal. `Ghostty.load()` would give that too, but decodes and compiles the ~420 KB
 * module per call, which delays every pane's first frame and its focus; compiling once and
 * instantiating per terminal keeps a pane ready as soon as it mounts.
 */

import { Ghostty } from "ghostty-web";
import ghosttyWasmUrl from "ghostty-web/ghostty-vt.wasm?url";

let compiled: Promise<WebAssembly.Module> | null = null;

function compiledModule(): Promise<WebAssembly.Module> {
  if (compiled === null) {
    compiled = fetch(ghosttyWasmUrl)
      .then((response) => {
        if (!response.ok) {
          throw new Error(`Failed to fetch the Ghostty WASM: ${response.status} ${response.statusText}`);
        }
        return response.arrayBuffer();
      })
      .then((bytes) => WebAssembly.compile(bytes));
    // A failed fetch or compile is not cached: the next terminal asks again rather than inheriting
    // the failure for the life of the page.
    compiled.catch(() => {
      compiled = null;
    });
  }
  return compiled;
}

/** A fresh Ghostty instance with its own memory, for exactly one terminal. */
export async function newGhosttyInstance(): Promise<Ghostty> {
  const module = await compiledModule();
  // The one import the module declares, as ghostty-web's own loader wires it: a log line written
  // into the instance's memory.
  let memory: WebAssembly.Memory | null = null;
  const instance = await WebAssembly.instantiate(module, {
    env: {
      log: (ptr: number, len: number) => {
        if (memory === null) return;
        console.log("[ghostty-vt]", new TextDecoder().decode(new Uint8Array(memory.buffer, ptr, len)));
      },
    },
  });
  memory = instance.exports.memory as WebAssembly.Memory;
  return new Ghostty(instance);
}

/** The bridge to the Rust core.
 *
 * Three modes, in order of preference:
 *
 * 1. **Desktop** — inside the Tauri window, two commands do the work.
 * 2. **Static** — the demo build embeds a snapshot of the manifest and a few
 *    captured outputs in the page. It runs the real interface so you can see
 *    how it behaves, and says plainly that the work itself happens natively.
 * 3. **Browser** — `npm run dev` with no shell. The module rail appears, the
 *    operations do not, because there is no core to ask.
 *
 * The Tauri modules are imported lazily so a browser or demo build never
 * evaluates them at all.
 */

import type { ModuleView, RunRequest, RunResult } from "./types";
import { FALLBACK_MODULES } from "./fallback";

interface StaticData {
  modules: ModuleView[];
  captured?: Record<string, string>;
}

declare global {
  interface Window {
    __SAVEHORSE_DATA__?: StaticData;
  }
}

export const isDesktop: boolean =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const isStatic: boolean =
  typeof window !== "undefined" && Boolean(window.__SAVEHORSE_DATA__);

export async function loadManifest(): Promise<ModuleView[]> {
  if (isDesktop) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<ModuleView[]>("manifest");
  }
  if (isStatic && window.__SAVEHORSE_DATA__) {
    return window.__SAVEHORSE_DATA__.modules;
  }
  return FALLBACK_MODULES;
}

export async function runOperation(request: RunRequest): Promise<RunResult> {
  if (isDesktop) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<RunResult>("run_operation", { request });
  }

  if (isStatic) {
    const data = window.__SAVEHORSE_DATA__;
    const key = `${request.module}.${request.op}`;
    const captured = data?.captured?.[key];
    if (captured) {
      return {
        text: captured,
        // The snapshot holds the text report only. Dressing that text up as
        // a JSON object made the tab look like broken output, so it says what
        // is actually true instead.
        json:
          "This preview captured the text report only.\n\n" +
          "JSON output comes from the Rust core — run the operation in the " +
          "app to see it.",
        dry_run: captured.toLowerCase().includes("preview"),
      };
    }
    throw new Error(
      "This page is a static preview of the interface, so the operation cannot run here. " +
        "Build the app to run it for real — see the README.",
    );
  }

  throw new Error(
    "The core is not connected. Run `npm run tauri dev` to execute operations.",
  );
}

/** The bridge to the Rust core.
 *
 * Inside the Tauri window this calls the two commands the app exposes. In a
 * plain browser — `npm run dev` without the shell — there is no core to call,
 * so the interface loads a small stand-in manifest and says so plainly rather
 * than pretending. The Tauri modules are imported lazily so a browser build
 * never evaluates them at all.
 */

import type { ModuleView, RunRequest, RunResult } from "./types";
import { FALLBACK_MODULES } from "./fallback";

export const isDesktop: boolean =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function loadManifest(): Promise<ModuleView[]> {
  if (!isDesktop) return FALLBACK_MODULES;
  const { invoke } = await import("@tauri-apps/api/core");
  return await invoke<ModuleView[]>("manifest");
}

export async function runOperation(request: RunRequest): Promise<RunResult> {
  if (!isDesktop) {
    throw new Error(
      "The core is not connected. Run `npm run tauri dev` to execute operations.",
    );
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return await invoke<RunResult>("run_operation", { request });
}

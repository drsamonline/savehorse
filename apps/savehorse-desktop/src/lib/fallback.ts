/** Stand-in module list for running the interface in a plain browser.
 *
 * Only names, titles and jobs — never operations. The real manifest, with all
 * 88 operations and their parameters, comes from the Rust registry. Copying it
 * here would create a second source of truth and it would drift within a week.
 */

import type { ModuleView } from "./types";

const m = (name: string, title: string, job: string, sources: string): ModuleView => ({
  name,
  title,
  job,
  sources,
  ops: [],
});

export const FALLBACK_MODULES: ModuleView[] = [
  m("aegis", "Vault & Security", "Encrypt, shred, scan and audit", "aegis + vault"),
  m("forge", "Binary Lab", "Inspect and patch executables", "forge"),
  m("halo", "Disc Studio", "Author, read and convert disc images", "halo + optic"),
  m("iris", "Image Studio", "Inspect, convert and transform images", "iris + prism"),
  m("arc", "Archives", "Pack, split, join and transfer", "arc + vortex"),
  m("sift", "Text & Search", "Find, extract and replace across files", "sift + slate"),
  m("glyph", "Documents", "PDF, Markdown and readability", "glyph"),
  m("scout", "Filesystem", "Catalogue, deduplicate and compare", "scout + strata"),
  m("morph", "Rename Engine", "Batch rename with preview and undo", "morph + strata"),
  m("pulse", "System", "Hardware, processes and maintenance", "pulse + beacon"),
  m("pager", "Window Manager", "Snap, pin and arrange windows", "pager + dock"),
  m("revive", "Recovery", "Carve, recover and verify snapshots", "revive + chronos"),
  m("flux", "Audio", "Inspect, tag and shape audio", "flux"),
];

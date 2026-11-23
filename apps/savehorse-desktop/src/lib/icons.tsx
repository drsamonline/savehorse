/** One glyph per module. Drawn as strokes on a 24px grid so they inherit the
 *  module colour and stay legible at 18px. */

import type { JSX } from "react";

const S = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.6,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export const MODULE_ICONS: Record<string, JSX.Element> = {
  // shield
  aegis: (
    <svg viewBox="0 0 24 24" {...S}>
      <path d="M12 3 19 6v6c0 4.2-2.9 7.3-7 9-4.1-1.7-7-4.8-7-9V6l7-3Z" />
      <path d="m9 12 2 2 4-4" />
    </svg>
  ),
  // anvil
  forge: (
    <svg viewBox="0 0 24 24" {...S}>
      <path d="M3 9h9l3-3h6v3h-3l-2 3H8" />
      <path d="M7 15h10M8 15v5M16 15v5" />
    </svg>
  ),
  // disc
  halo: (
    <svg viewBox="0 0 24 24" {...S}>
      <circle cx="12" cy="12" r="8.5" />
      <circle cx="12" cy="12" r="2.5" />
      <path d="M12 3.5v3M12 17.5v3" />
    </svg>
  ),
  // picture
  iris: (
    <svg viewBox="0 0 24 24" {...S}>
      <rect x="3.5" y="5" width="17" height="14" rx="2" />
      <circle cx="9" cy="10" r="1.6" />
      <path d="m4 17 5-4 4 3 3-2 4 3" />
    </svg>
  ),
  // crate
  arc: (
    <svg viewBox="0 0 24 24" {...S}>
      <path d="M12 3.5 20 8v8l-8 4.5L4 16V8l8-4.5Z" />
      <path d="M4 8l8 4.5L20 8M12 12.5V21" />
    </svg>
  ),
  // magnifier
  sift: (
    <svg viewBox="0 0 24 24" {...S}>
      <circle cx="11" cy="11" r="6.5" />
      <path d="m16 16 4.5 4.5" />
    </svg>
  ),
  // document
  glyph: (
    <svg viewBox="0 0 24 24" {...S}>
      <path d="M6 3.5h8l4 4v13H6v-17Z" />
      <path d="M14 3.5v4h4M9 12h6M9 16h4" />
    </svg>
  ),
  // folder tree
  scout: (
    <svg viewBox="0 0 24 24" {...S}>
      <path d="M3.5 6.5h6l1.5 2h9v11h-16v-13Z" />
      <path d="M7 12h10M7 15.5h6" />
    </svg>
  ),
  // tag
  morph: (
    <svg viewBox="0 0 24 24" {...S}>
      <path d="M12.5 3.5H20v7.5l-9 9-7.5-7.5 9-9Z" />
      <circle cx="16.5" cy="7.5" r="1.4" />
    </svg>
  ),
  // gauge
  pulse: (
    <svg viewBox="0 0 24 24" {...S}>
      <path d="M4 17a8 8 0 1 1 16 0" />
      <path d="m12 17 4-5" />
      <circle cx="12" cy="17" r="1.3" />
    </svg>
  ),
  // grid
  pager: (
    <svg viewBox="0 0 24 24" {...S}>
      <rect x="3.5" y="3.5" width="7" height="7" rx="1.4" />
      <rect x="13.5" y="3.5" width="7" height="7" rx="1.4" />
      <rect x="3.5" y="13.5" width="7" height="7" rx="1.4" />
      <rect x="13.5" y="13.5" width="7" height="7" rx="1.4" />
    </svg>
  ),
  // rewind
  revive: (
    <svg viewBox="0 0 24 24" {...S}>
      <path d="M12 6.5a6.5 6.5 0 1 1-6.4 7.8" />
      <path d="M5.5 6.5v4h4" />
    </svg>
  ),
  // waveform
  flux: (
    <svg viewBox="0 0 24 24" {...S}>
      <path d="M3 12h2.5l2-5 3 10 3-13 3 11 2-3H21" />
    </svg>
  ),
};

export function moduleIcon(name: string): JSX.Element {
  return MODULE_ICONS[name] ?? MODULE_ICONS.sift;
}

import { useEffect, useMemo, useRef, useState } from "react";
import type { ModuleView } from "../lib/types";

interface Props {
  modules: ModuleView[];
  onPick: (module: string, op: string) => void;
  onClose: () => void;
}

interface Row {
  module: string;
  title: string;
  op: string;
  summary: string;
  planned: boolean;
}

/** The command palette: the primary way in. Type what you want to do and the
 *  manifest does the finding — all 88 operations, no menus to learn. */
export function Palette({ modules, onPick, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const rows = useMemo<Row[]>(() => {
    const all: Row[] = [];
    for (const m of modules) {
      for (const o of m.ops) {
        all.push({
          module: m.name,
          title: m.title,
          op: o.name,
          summary: o.summary,
          planned: o.status === "planned",
        });
      }
    }
    return all;
  }, [modules]);

  const matches = useMemo<Row[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    const words = q.split(/\s+/);
    return rows.filter((r) => {
      const hay = `${r.module} ${r.op} ${r.summary} ${r.title}`.toLowerCase();
      return words.every((w) => hay.includes(w));
    });
  }, [rows, query]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    setCursor(0);
  }, [query]);

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>('[data-active="true"]');
    el?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor((c) => Math.min(c + 1, Math.max(matches.length - 1, 0)));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor((c) => Math.max(c - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const row = matches[cursor];
      if (row) onPick(row.module, row.op);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    }
  }

  return (
    <div className="scrim" onMouseDown={onClose} role="presentation">
      <div
        className="palette"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          // A modal with one focusable control: Tab must not walk out of it.
          if (e.key === "Tab") e.preventDefault();
        }}
        role="dialog"
        aria-modal="true"
        aria-label="Search operations"
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Search operations — try shred, carve, iso, duplicate"
          aria-label="Search operations"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-autocomplete="list"
          aria-activedescendant={
            matches.length > 0 ? `palette-opt-${cursor}` : undefined
          }
        />
        <div className="palette-list" ref={listRef} id="palette-list" role="listbox"
          aria-label="Operations">
          {matches.length === 0 && (
            <div className="palette-row" style={{ color: "var(--ink-faint)" }}>
              Nothing matches “{query}”.
            </div>
          )}
          {matches.map((r, i) => (
            <button
              key={`${r.module}.${r.op}`}
              id={`palette-opt-${i}`}
              className="palette-row"
              data-active={i === cursor}
              role="option"
              aria-selected={i === cursor}
              // The rows are reached with the arrow keys, not with Tab: a list
              // of 87 tabbable buttons is a trap, not a convenience.
              tabIndex={-1}
              onMouseEnter={() => setCursor(i)}
              onClick={() => onPick(r.module, r.op)}
            >
              <span className="mod">{r.module}</span>
              <span className="op">{r.op}</span>
              <span className="sum">{r.summary}</span>
              {r.planned && <span className="badge">planned</span>}
            </button>
          ))}
        </div>
        <div className="palette-foot">
          <span>↑↓ move</span>
          <span>↵ open</span>
          <span>esc close</span>
          <span style={{ marginLeft: "auto" }}>
            {matches.length} of {rows.length}
          </span>
        </div>
      </div>
    </div>
  );
}

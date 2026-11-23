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
        />
        <div className="palette-list" ref={listRef}>
          {matches.length === 0 && (
            <div className="palette-row" style={{ color: "var(--ink-faint)" }}>
              Nothing matches “{query}”.
            </div>
          )}
          {matches.map((r, i) => (
            <button
              key={`${r.module}.${r.op}`}
              className="palette-row"
              data-active={i === cursor}
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

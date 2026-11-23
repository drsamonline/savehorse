import { useState } from "react";
import type { RunResult } from "../lib/types";

interface Props {
  result: RunResult | null;
  error: string | null;
  ms: number | null;
  running: boolean;
}

/** The result pane. Text for a person, JSON for whatever comes next — the same
 *  operation produces both, so the pane can show either without re-running. */
export function ResultPane({ result, error, ms, running }: Props) {
  const [tab, setTab] = useState<"text" | "json">("text");

  const showJson = tab === "json" && result;
  const body = error
    ? error
    : showJson
      ? result.json
      : (result?.text ?? "");

  return (
    <section className="result" aria-label="Result">
      <div className="result-head">
        <button
          className="tab"
          data-active={tab === "text"}
          onClick={() => setTab("text")}
        >
          Text
        </button>
        <button
          className="tab"
          data-active={tab === "json"}
          onClick={() => setTab("json")}
          disabled={!result}
        >
          JSON
        </button>

        <div className="result-meta">
          {running && <span>running…</span>}
          {error && <span style={{ color: "var(--danger)" }}>failed</span>}
          {result?.dry_run && !error && (
            <span style={{ color: "var(--warn)" }}>preview — nothing changed</span>
          )}
          {ms !== null && !running && <span>{ms} ms</span>}
          {result && <span>{countLines(body)} lines</span>}
        </div>
      </div>

      {body ? (
        <pre className="result-body">{body}</pre>
      ) : (
        <div className="empty">
          <div className="big">{running ? "Working…" : "Nothing yet"}</div>
          <div className="small">
            {running
              ? "The result will appear here."
              : "Run an operation and its output lands here. Every operation also produces JSON — switch tabs to see it."}
          </div>
        </div>
      )}
    </section>
  );
}

function countLines(s: string): number {
  if (!s) return 0;
  return s.split("\n").length;
}

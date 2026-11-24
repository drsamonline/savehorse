import { useEffect, useState } from "react";
import type { RunResult } from "../lib/types";
import { moduleIcon } from "../lib/icons";

interface Props {
  result: RunResult | null;
  error: string | null;
  ms: number | null;
  running: boolean;
  /** The active module, so an empty pane still says whose it is. */
  moduleName?: string;
}

/** The result pane. Text for a person, JSON for whatever comes next — the same
 *  operation produces both, so the pane can show either without re-running. */
export function ResultPane({ result, error, ms, running, moduleName }: Props) {
  const moduleIconNode = moduleName ? moduleIcon(moduleName) : null;
  const [tab, setTab] = useState<"text" | "json">("text");
  const [copied, setCopied] = useState(false);
  const [elapsed, setElapsed] = useState(0);

  // A clock while an operation runs. Operations do not report progress — the
  // runner returns a finished result — so a percentage would be invented. What
  // can be told honestly is how long it has been going.
  useEffect(() => {
    if (!running) {
      setElapsed(0);
      return;
    }
    const started = Date.now();
    const timer = window.setInterval(() => setElapsed(Date.now() - started), 100);
    return () => window.clearInterval(timer);
  }, [running]);

  const showJson = tab === "json" && result;
  const body = error
    ? error
    : showJson
      ? result.json
      : (result?.text ?? "");

  async function copy() {
    if (!body) return;
    try {
      await navigator.clipboard.writeText(body);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    } catch {
      // Clipboard access can be refused; the button simply does nothing.
    }
  }

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

        {body && !running && (
          <button className="copy" onClick={() => void copy()} title="Copy the output">
            {copied ? "copied" : "copy"}
          </button>
        )}

        <div className="result-meta">
          {running && <span>{(elapsed / 1000).toFixed(1)}s</span>}
          {error && <span style={{ color: "var(--danger)" }}>failed</span>}
          {result?.dry_run && !error && (
            <span style={{ color: "var(--warn)" }}>preview — nothing changed</span>
          )}
          {ms !== null && !running && <span>{ms} ms</span>}
          {result && <span>{countLines(body)} lines</span>}
        </div>
      </div>

      {running && (
        <div className="progress" role="progressbar" aria-label="Operation running">
          <span />
        </div>
      )}

      {body ? (
        <pre className="result-body">{body}</pre>
      ) : (
        <div className="empty">
          {moduleIconNode && (
            <div className="empty-mark" aria-hidden="true">
              {moduleIconNode}
            </div>
          )}
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

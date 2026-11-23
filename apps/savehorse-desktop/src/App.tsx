import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Job, ModuleView, OpView, RunRequest, RunResult } from "./lib/types";
import { isDesktop, isStatic, loadManifest, runOperation } from "./lib/bridge";
import { Rail } from "./components/Rail";
import { Palette } from "./components/Palette";
import { OpForm } from "./components/OpForm";
import { ResultPane } from "./components/ResultPane";
import { JobDock } from "./components/JobDock";

const MAX_JOBS = 20;

function defaultValues(op: OpView | null): Record<string, string> {
  const values: Record<string, string> = {};
  if (!op) return values;
  for (const p of op.params) {
    if (p.kind === "Bool") values[p.name] = "false";
    else if (p.kind === "Choice" && p.options.length > 0) values[p.name] = p.options[0];
    else values[p.name] = "";
  }
  return values;
}

function buildRequest(
  module: string,
  op: OpView,
  values: Record<string, string>,
  apply: boolean,
): RunRequest {
  const positional: string[] = [];
  const flags: Record<string, string> = {};

  for (const p of op.params) {
    const raw = (values[p.name] ?? "").trim();
    if (p.positional) {
      positional.push(raw);
    } else if (p.kind === "Bool") {
      if (raw === "true") flags[p.name] = "true";
    } else if (raw !== "") {
      flags[p.name] = raw;
    }
  }

  // Anything that writes is previewed unless the user explicitly opts in.
  // Operations that carry their own `commit` switch handle it themselves.
  const hasCommit = op.params.some((p) => p.name === "commit");
  if (op.danger !== "safe" && !hasCommit) {
    flags["dry-run"] = apply ? "false" : "true";
  }

  return { module, op: op.name, positional, flags };
}

function cliEcho(module: string, op: OpView, values: Record<string, string>, apply: boolean): string {
  const parts = ["savehorse", module, op.name];

  for (const p of op.params) {
    if (!p.positional) continue;
    const v = (values[p.name] ?? "").trim();
    parts.push(v === "" ? `<${p.name}>` : quote(v));
  }
  for (const p of op.params) {
    if (p.positional) continue;
    const v = (values[p.name] ?? "").trim();
    if (p.kind === "Bool") {
      if (v === "true") parts.push(`--${p.name}`);
    } else if (v !== "") {
      parts.push(`--${p.name} ${quote(v)}`);
    }
  }

  const hasCommit = op.params.some((p) => p.name === "commit");
  if (op.danger !== "safe" && !hasCommit && !apply) parts.push("--dry-run");

  return parts.join(" ");
}

function quote(s: string): string {
  return /[\s"']/.test(s) ? `"${s.replace(/"/g, '\\"')}"` : s;
}

export default function App() {
  const [modules, setModules] = useState<ModuleView[]>([]);
  const [activeModule, setActiveModule] = useState("sift");
  const [activeOp, setActiveOp] = useState<string | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [apply, setApply] = useState(false);
  const [result, setResult] = useState<RunResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ms, setMs] = useState<number | null>(null);
  const [running, setRunning] = useState(false);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const nextJobId = useRef(1);
  // Values the form should take on the next reset (used by the static demo).
  const pendingValues = useRef<Record<string, string> | null>(null);

  // Load the manifest once. In the desktop app it comes from the Rust registry;
  // in a browser it is a names-only stand-in.
  useEffect(() => {
    let cancelled = false;
    loadManifest()
      .then((list) => {
        if (cancelled) return;
        setModules(list);
        // The static demo opens on an operation it has a captured result for,
        // so the page shows real output rather than an empty pane.
        const showcase = isStatic
          ? list.find((m) => m.name === "sift")?.ops.find((o) => o.name === "search")
          : undefined;

        if (showcase) {
          const sample = { pattern: "alpha", paths: "smoke" };
          pendingValues.current = sample;
          setActiveModule("sift");
          setActiveOp("search");
          void run({ module: "sift", op: showcase, values: sample, apply: false });
          return;
        }

        const first = list.find((m) => m.ops.some((o) => o.status === "implemented")) ?? list[0];
        if (first) {
          setActiveModule(first.name);
          const op = first.ops.find((o) => o.status === "implemented") ?? first.ops[0];
          if (op) setActiveOp(op.name);
        }
      })
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  const currentModule = useMemo(
    () => modules.find((m) => m.name === activeModule) ?? null,
    [modules, activeModule],
  );

  const currentOp = useMemo(
    () => currentModule?.ops.find((o) => o.name === activeOp) ?? null,
    [currentModule, activeOp],
  );

  // Keep the accent colour in step with the module.
  useEffect(() => {
    document.documentElement.dataset.module = activeModule;
  }, [activeModule]);

  // Reset the form whenever the operation changes.
  useEffect(() => {
    if (pendingValues.current) {
      setValues(pendingValues.current);
      pendingValues.current = null;
    } else {
      setValues(defaultValues(currentOp));
    }
    setApply(false);
    setError(null);
  }, [currentOp]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 2600);
    return () => clearTimeout(t);
  }, [toast]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const selectModule = useCallback(
    (name: string) => {
      setActiveModule(name);
      const m = modules.find((x) => x.name === name);
      const op = m?.ops.find((o) => o.status === "implemented") ?? m?.ops[0];
      setActiveOp(op ? op.name : null);
      setResult(null);
      setMs(null);
    },
    [modules],
  );

  const selectOp = useCallback(
    (moduleName: string, opName: string) => {
      setActiveModule(moduleName);
      setActiveOp(opName);
      setResult(null);
      setMs(null);
      setPaletteOpen(false);
    },
    [],
  );

  const run = useCallback(
    async (override?: { module: string; op: OpView; values: Record<string, string>; apply: boolean }) => {
      const module = override?.module ?? activeModule;
      const op = override?.op ?? currentOp;
      const vals = override?.values ?? values;
      const applyFlag = override?.apply ?? apply;
      if (!op || op.status !== "implemented") return;

      const request = buildRequest(module, op, vals, applyFlag);
      const started = performance.now();
      setRunning(true);
      setError(null);

      const stamp = new Date().toLocaleTimeString([], { hour12: false });

      try {
        const res = await runOperation(request);
        const elapsed = Math.round(performance.now() - started);
        setResult(res);
        setMs(elapsed);
        setJobs((prev) =>
          [
            {
              id: nextJobId.current++,
              at: stamp,
              label: `${module}.${op.name}`,
              state: res.dry_run ? ("preview" as const) : ("ok" as const),
              detail: firstLine(res.text),
              ms: elapsed,
              request,
            },
            ...prev,
          ].slice(0, MAX_JOBS),
        );
      } catch (e: unknown) {
        const message = e instanceof Error ? e.message : String(e);
        const elapsed = Math.round(performance.now() - started);
        setError(message);
        setResult(null);
        setMs(elapsed);
        setJobs((prev) =>
          [
            {
              id: nextJobId.current++,
              at: stamp,
              label: `${module}.${op.name}`,
              state: "error" as const,
              detail: message,
              ms: elapsed,
              request,
            },
            ...prev,
          ].slice(0, MAX_JOBS),
        );
      } finally {
        setRunning(false);
      }
    },
    [activeModule, currentOp, values, apply],
  );

  function replay(job: Job) {
    const module = modules.find((m) => m.name === job.request.module);
    const op = module?.ops.find((o) => o.name === job.request.op);
    if (!module || !op) return;

    // Rebuild the form values from the stored request so the panels agree.
    const next: Record<string, string> = defaultValues(op);
    let i = 0;
    for (const p of op.params) {
      if (p.positional) {
        if (i < job.request.positional.length) next[p.name] = job.request.positional[i];
        i += 1;
      } else if (job.request.flags[p.name] !== undefined) {
        next[p.name] = job.request.flags[p.name];
      }
    }
    const replayApply = job.request.flags["dry-run"] === "false";
    setActiveModule(module.name);
    setActiveOp(op.name);
    setValues(next);
    setApply(replayApply);
    void run({ module: module.name, op, values: next, apply: replayApply });
  }

  function onDrop(e: React.DragEvent) {
    e.preventDefault();
    setDragging(false);
    const file = e.dataTransfer.files?.[0];
    if (!file || !currentOp) return;
    const target = currentOp.params.find((p) => p.kind === "Path" || p.kind === "Paths");
    if (!target) {
      setToast("This operation does not take a file.");
      return;
    }
    setValues((v) => ({ ...v, [target.name]: file.name }));
    setToast(`Filled ${target.name} with ${file.name}`);
  }

  const canRun = Boolean(
    currentOp &&
      currentOp.status === "implemented" &&
      currentOp.params
        .filter((p) => p.positional && p.required)
        .every((p) => (values[p.name] ?? "").trim() !== ""),
  );

  const echo = currentOp
    ? cliEcho(activeModule, currentOp, values, apply)
    : "savehorse <module> <op>";

  const hasCommit = currentOp?.params.some((p) => p.name === "commit") ?? false;
  const showApply = Boolean(currentOp && currentOp.danger !== "safe" && !hasCommit);

  return (
    <div className="app">
      <Rail modules={modules} active={activeModule} onSelect={selectModule} />

      <header className="topbar">
        <h1>
          {currentModule?.title ?? "savehorse"}
          <span>{currentModule?.job ?? "one bench, thirteen tools"}</span>
        </h1>
        <button className="searchbtn" onClick={() => setPaletteOpen(true)}>
          <span>Search operations</span>
          <span className="kbd">⌘K</span>
        </button>
      </header>

      {!isDesktop && (
        <div className="banner">
          {isStatic
            ? "Static preview — this is the real interface with a snapshot of the manifest. Operations marked with a captured result show what the native app produced; the rest run only in the app."
            : "Preview mode — the interface is running without the Rust core, so the operation list is empty. Run npm run tauri dev for the real thing."}
        </div>
      )}

      <div
        className="main"
        data-dragging={dragging}
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={onDrop}
      >
        <section className="panel" aria-label="Operations">
          <div className="panel-head">
            <div className="eyebrow">{currentModule?.name ?? ""}</div>
            <h2>{currentModule?.title ?? ""}</h2>
            <p>{currentModule?.job ?? ""}</p>
          </div>
          <div className="op-list">
            {(currentModule?.ops ?? []).map((o) => (
              <button
                key={o.id}
                className="op-row"
                data-active={o.name === activeOp}
                data-planned={o.status === "planned"}
                onClick={() => selectOp(activeModule, o.name)}
              >
                <span className="name">{o.name}</span>
                <span className="sum">{o.summary}</span>
              </button>
            ))}
            {(currentModule?.ops.length ?? 0) === 0 && (
              <div className="op-row">
                <span className="sum">
                  No operations loaded. The manifest comes from the Rust core.
                </span>
              </div>
            )}
          </div>
          <OpForm
            op={currentOp}
            values={values}
            onChange={(name, value) => setValues((v) => ({ ...v, [name]: value }))}
            onRun={() => void run()}
            running={running}
            cliEcho={echo}
            canRun={canRun}
            showApply={showApply}
            apply={apply}
            onApplyChange={setApply}
          />
        </section>

        <ResultPane result={result} error={error} ms={ms} running={running} />
      </div>

      <JobDock jobs={jobs} onReplay={replay} />

      {paletteOpen && (
        <Palette modules={modules} onPick={selectOp} onClose={() => setPaletteOpen(false)} />
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}

function firstLine(text: string): string {
  const line = text.split("\n").find((l) => l.trim() !== "") ?? "";
  return line.length > 90 ? `${line.slice(0, 87)}…` : line;
}

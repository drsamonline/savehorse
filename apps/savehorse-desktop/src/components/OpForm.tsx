import type { OpView, ParamView } from "../lib/types";
import { DANGER_LABEL } from "../lib/types";

interface Props {
  op: OpView | null;
  values: Record<string, string>;
  onChange: (name: string, value: string) => void;
  onRun: () => void;
  running: boolean;
  cliEcho: string;
  canRun: boolean;
  /** Show the "apply for real" switch for operations that write. */
  showApply: boolean;
  apply: boolean;
  onApplyChange: (v: boolean) => void;
}

function Field({
  p,
  value,
  onChange,
}: {
  p: ParamView;
  value: string;
  onChange: (v: string) => void;
}) {
  const id = `f-${p.name}`;

  if (p.kind === "Bool") {
    return (
      <label className="switch" htmlFor={id}>
        <input
          id={id}
          type="checkbox"
          checked={value === "true"}
          onChange={(e) => onChange(e.target.checked ? "true" : "false")}
        />
        <span>{p.name}</span>
        <span className="hint" style={{ color: "var(--ink-faint)", fontSize: 10 }}>
          {p.help}
        </span>
      </label>
    );
  }

  if (p.kind === "Choice" && p.options.length > 0) {
    return (
      <div className="field">
        <label htmlFor={id}>
          {p.name}
          {p.required && <span className="req">*</span>}
          <span className="hint">{p.help}</span>
        </label>
        <select
          id={id}
          className="input"
          value={value || p.options[0]}
          onChange={(e) => onChange(e.target.value)}
        >
          {p.options.map((o) => (
            <option key={o} value={o}>
              {o}
            </option>
          ))}
        </select>
      </div>
    );
  }

  const placeholder =
    p.kind === "Path" || p.kind === "Paths"
      ? "drop a file here, or type a path"
      : p.kind === "Size"
        ? "e.g. 10k"
        : p.kind === "Number"
          ? "e.g. 4"
          : p.kind === "Secret"
            ? "not stored, not logged"
            : "";

  return (
    <div className="field">
      <label htmlFor={id}>
        {p.name}
        {p.required && <span className="req">*</span>}
        <span className="hint">{p.help}</span>
      </label>
      <input
        id={id}
        className="input"
        type={p.kind === "Secret" ? "password" : "text"}
        value={value}
        placeholder={placeholder}
        spellCheck={false}
        autoComplete="off"
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}

/** The operation form, rendered from the manifest rather than written by hand.
 *  Every operation gets the right control, the right validation hint and the
 *  right warning without a screen of bespoke code. */
export function OpForm({
  op,
  values,
  onChange,
  onRun,
  running,
  cliEcho,
  canRun,
  showApply,
  apply,
  onApplyChange,
}: Props) {
  if (!op) {
    return (
      <div className="empty">
        <div className="big">Pick an operation</div>
        <div className="small">
          Choose from the list, or press ⌘K to search all of them.
        </div>
      </div>
    );
  }

  const planned = op.status === "planned";
  const destructive = op.danger !== "safe";

  return (
    <div className="form">
      {op.params.length === 0 && !planned && (
        <div className="small" style={{ color: "var(--ink-faint)" }}>
          This operation takes no parameters.
        </div>
      )}

      {op.params.map((p) => (
        <Field
          key={p.name}
          p={p}
          value={values[p.name] ?? ""}
          onChange={(v) => onChange(p.name, v)}
        />
      ))}

      {showApply && (
        <label className="switch">
          <input
            type="checkbox"
            checked={apply}
            onChange={(e) => onApplyChange(e.target.checked)}
          />
          <span>apply for real</span>
          <span className="hint" style={{ color: "var(--ink-faint)", fontSize: 10 }}>
            off means preview: the run reports what it would change and changes nothing
          </span>
        </label>
      )}

      <div className="actions">
        <button className="run" onClick={onRun} disabled={running || !canRun || planned}>
          {planned ? "Not ported yet" : running ? "Running…" : destructive ? "Preview" : "Run"}
        </button>
        {destructive && !planned && (
          <span className="danger-note" data-level={op.danger.toUpperCase()}>
            {op.danger === "irreversible" ? "▲" : "•"} {DANGER_LABEL[op.danger]}
            {op.danger === "irreversible" ? " — previews first, deletes only on confirm" : ""}
          </span>
        )}
      </div>

      <div className="cli-echo" title="The same thing, on the command line">
        <b>$</b> {cliEcho}
      </div>
    </div>
  );
}

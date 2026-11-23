import type { ModuleView } from "../lib/types";
import { moduleIcon } from "../lib/icons";

interface Props {
  modules: ModuleView[];
  active: string;
  onSelect: (name: string) => void;
}

/** The module rail. Thirteen tools, always in the same order, each with the
 *  colour it keeps everywhere else in the app. */
export function Rail({ modules, active, onSelect }: Props) {
  return (
    <nav className="rail" aria-label="Modules">
      <div className="rail-mark" title="savehorse">
        S
      </div>
      {modules.map((m) => (
        <button
          key={m.name}
          className="rail-btn"
          data-active={m.name === active}
          data-module={m.name}
          onClick={() => onSelect(m.name)}
          title={`${m.title} — ${m.job}`}
          aria-label={`${m.title}: ${m.job}`}
          aria-current={m.name === active ? "page" : undefined}
        >
          {moduleIcon(m.name)}
        </button>
      ))}
    </nav>
  );
}

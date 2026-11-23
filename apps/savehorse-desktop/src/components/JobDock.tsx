import type { Job } from "../lib/types";

interface Props {
  jobs: Job[];
  onReplay: (job: Job) => void;
}

/** The job dock. Every run is kept, so a good operation can be re-run with one
 *  click — which is the first half of the recipe idea. */
export function JobDock({ jobs, onReplay }: Props) {
  return (
    <section className="dock" aria-label="Jobs">
      <div className="dock-head">
        <span>Jobs</span>
        <span className="count">{jobs.length}</span>
        <span style={{ marginLeft: "auto", textTransform: "none" }}>
          {jobs.length > 0 ? "click a job to run it again" : ""}
        </span>
      </div>
      <div className="dock-list">
        {jobs.length === 0 && (
          <div className="job">
            <span className="time">—</span>
            <span className="id">no runs yet</span>
            <span className="status">this session keeps the last twenty</span>
            <span className="dur" />
          </div>
        )}
        {jobs.map((j) => (
          <button
            key={j.id}
            className="job"
            onClick={() => onReplay(j)}
            title="Run this again"
          >
            <span className="time">{j.at}</span>
            <span className="id">{j.label}</span>
            <span className="status" data-state={j.state}>
              {j.detail}
            </span>
            <span className="dur">{j.ms} ms</span>
          </button>
        ))}
      </div>
    </section>
  );
}

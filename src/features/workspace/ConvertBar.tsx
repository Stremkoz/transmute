import type { Overview, PlanSummary } from "@bindings";
import { formatCount, plural } from "../../lib/units";
import { isRunActive } from "../../state/session";
import { useSession } from "../../state/sessionContext";
import styles from "./ConvertBar.module.css";

/** Why there's nothing to plan yet, if there isn't. */
function blocker({ inputs, preset }: Overview): string | null {
  if (inputs.roots.length === 0) return "Add images to get started.";
  if (inputs.convertible === 0) return "None of these files are images Transmute converts.";
  if (inputs.convertible <= inputs.excluded) return "Every image is excluded.";
  if (preset.outputs.length === 0) return "Pick an output format.";
  return null;
}

function PlanLine({ plan }: { plan: PlanSummary }) {
  const extras: string[] = [];
  if (plan.skipped > 0) extras.push(`${formatCount(plan.skipped)} skipped`);
  if (plan.blocked > 0) extras.push(`${formatCount(plan.blocked)} can't be converted`);
  return (
    <span>
      {plural(plan.sources, "image")} × {plural(plan.outputsPerSource, "output")} →{" "}
      <strong>{plural(plan.ready, "file")}</strong> to write
      {extras.length > 0 && <span className={styles.extras}> · {extras.join(" · ")}</span>}
    </span>
  );
}

/** Problems and notes, grouped with counts (expandable). */
function PlanDetails({ plan }: { plan: PlanSummary }) {
  const items = [...plan.problems, ...plan.notes];
  if (items.length === 0) return null;
  return (
    <details className={styles.details}>
      <summary>Details</summary>
      <ul>
        {items.map((item) => (
          <li key={item.code + item.text}>
            {item.text} <span className={styles.count}>{formatCount(item.count)}</span>
          </li>
        ))}
      </ul>
    </details>
  );
}

function Status({ overview }: { overview: Overview }) {
  const planning = useSession((state) => state.planning);
  const planError = useSession((state) => state.planError);
  const reason = blocker(overview);
  if (reason) return <p className={styles.muted}>{reason}</p>;
  if (planning) return <p className={styles.muted}>Checking what will be converted…</p>;
  if (planError) return <p className={styles.error}>{planError}</p>;
  if (!overview.plan) return null;
  return (
    <div className={styles.plan}>
      <PlanLine plan={overview.plan} />
      <PlanDetails plan={overview.plan} />
    </div>
  );
}

export function ConvertBar({ overview }: { overview: Overview }) {
  const planning = useSession((state) => state.planning);
  const running = useSession((state) => isRunActive(state.run));
  const convert = useSession((state) => state.convert);
  const ready = blocker(overview) === null && !planning && (overview.plan?.ready ?? 0) > 0;

  return (
    <footer className={styles.bar}>
      <div className={styles.status} aria-live="polite">
        {running ? <p className={styles.muted}>Converting… edits wait until the run ends.</p> : <Status overview={overview} />}
      </div>
      <button
        type="button"
        className="button button-primary"
        disabled={!ready || running}
        onClick={() => void convert()}
      >
        Convert
      </button>
    </footer>
  );
}

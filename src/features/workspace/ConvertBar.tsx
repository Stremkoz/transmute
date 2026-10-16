import type { Overview } from "@bindings";
import styles from "./ConvertBar.module.css";

/** What the bar says the next run would do. The live plan arrives in G1. */
function planLine({ inputs, preset, plan }: Overview): string {
  if (inputs.roots.length === 0) return "Add images to get started.";
  if (preset.outputs.length === 0) return "Pick an output format.";
  if (!plan) return "Checking what will be converted…";
  return `${plan.sources.toLocaleString()} × ${plan.outputsPerSource} → ${plan.ready.toLocaleString()} files`;
}

export function ConvertBar({ overview }: { overview: Overview }) {
  const canConvert = overview.plan !== null && overview.plan.ready > 0;
  return (
    <footer className={styles.bar}>
      <p className={styles.plan} aria-live="polite">
        {planLine(overview)}
      </p>
      <button type="button" className="button button-primary" disabled={!canConvert}>
        Convert
      </button>
    </footer>
  );
}

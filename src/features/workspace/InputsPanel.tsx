import type { InputsSummary } from "@bindings";
import styles from "./InputsPanel.module.css";

/**
 * The left column. Empty, it's the drop zone; with inputs, a summary (the
 * tree arrives in G2). Dropping and the Add buttons are wired in G1.
 */
export function InputsPanel({ inputs }: { inputs: InputsSummary }) {
  return (
    <section className={styles.panel} aria-labelledby="inputs-heading">
      <h2 id="inputs-heading" className={styles.heading}>
        Inputs
      </h2>
      {inputs.roots.length === 0 ? (
        <div className={styles.dropZone}>
          <p className={styles.dropTitle}>Drop images or folders here</p>
          <p className={styles.dropHint}>Folders keep their structure. Nothing is changed until you convert.</p>
        </div>
      ) : (
        <p className={styles.summary}>
          {inputs.convertible.toLocaleString()} images in {inputs.roots.length.toLocaleString()}{" "}
          {inputs.roots.length === 1 ? "input" : "inputs"}
        </p>
      )}
    </section>
  );
}

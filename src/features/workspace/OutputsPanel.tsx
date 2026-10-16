import type { Destination, Preset } from "@bindings";
import { FORMAT_LABELS, outputSummary } from "../../lib/formats";
import styles from "./OutputsPanel.module.css";

function destinationLabel(destination: Destination | undefined): string {
  switch (destination?.mode) {
    case "subfolder":
      return "A subfolder per format, inside each source folder";
    case "custom":
      return destination.root;
    // Unset falls back to the user's defaults; G1 shows the layered value.
    default:
      return "Beside each source";
  }
}

/** The right column: one card per output, then where files will land. */
export function OutputsPanel({ preset }: { preset: Preset }) {
  return (
    <section className={styles.panel} aria-labelledby="outputs-heading">
      <h2 id="outputs-heading" className={styles.heading}>
        Outputs
      </h2>
      {preset.outputs.length === 0 ? (
        <p className={styles.empty}>Pick at least one format to convert to.</p>
      ) : (
        <ul className={styles.cards}>
          {preset.outputs.map((output, index) => (
            <li key={index} className={styles.card}>
              <span className={styles.format}>{FORMAT_LABELS[output.codec.format]}</span>
              <span className={styles.settings}>{outputSummary(output)}</span>
            </li>
          ))}
        </ul>
      )}
      <button type="button" className="button" disabled>
        Add output
      </button>
      <div className={styles.destination}>
        <h3 className={styles.subheading}>Destination</h3>
        <p>{destinationLabel(preset.destination)}</p>
      </div>
    </section>
  );
}

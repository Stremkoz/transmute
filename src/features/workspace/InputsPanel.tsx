import type { InputsSummary } from "@bindings";
import { FORMAT_LABELS } from "../../lib/formats";
import { formatCount, plural } from "../../lib/units";
import { isRunActive } from "../../state/session";
import { usePlatform, useSession } from "../../state/sessionContext";
import styles from "./InputsPanel.module.css";

function AddButtons({ disabled }: { disabled: boolean }) {
  const platform = usePlatform();
  const addInputs = useSession((state) => state.addInputs);
  const add = async (pick: () => Promise<string[]>) => {
    const paths = await pick();
    if (paths.length > 0) await addInputs(paths);
  };
  return (
    <>
      <button type="button" className="button" disabled={disabled} onClick={() => void add(platform.pickFiles)}>
        Add files
      </button>
      <button type="button" className="button" disabled={disabled} onClick={() => void add(platform.pickFolders)}>
        Add folder
      </button>
    </>
  );
}

function ScanStatus() {
  const scan = useSession((state) => state.scan);
  if (!scan) return null;
  return (
    <p className={styles.scan} role="status">
      Scanning… {plural(scan.discovered, "file")} found, {formatCount(scan.inspected)} checked
    </p>
  );
}

function Breakdown({ inputs }: { inputs: InputsSummary }) {
  const formats = [...inputs.byFormat].sort((a, b) => b.count - a.count);
  return (
    <div className={styles.breakdown}>
      <p>
        <strong>{plural(inputs.convertible, "image")}</strong> to convert
      </p>
      {formats.length > 0 && (
        <ul className={styles.formats} aria-label="Images by format">
          {formats.map(({ format, count }) => (
            <li key={format}>
              {FORMAT_LABELS[format]} <span className={styles.count}>{formatCount(count)}</span>
            </li>
          ))}
        </ul>
      )}
      {inputs.unsupported > 0 && (
        <p className={styles.muted}>
          {plural(inputs.unsupported, "other file")} {inputs.unsupported === 1 ? "isn't an image" : "aren't images"}{" "}
          Transmute converts, and will be left alone.
        </p>
      )}
      {inputs.warnings.length > 0 && (
        <ul className={styles.warnings}>
          {inputs.warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * The left column. Empty, it's the drop zone; with inputs, compact Add
 * actions, the dropped items and what they contain (the browsable tree
 * arrives in G2).
 */
export function InputsPanel({ inputs }: { inputs: InputsSummary }) {
  const removeRoot = useSession((state) => state.removeRoot);
  const clearInputs = useSession((state) => state.clearInputs);
  const scanning = useSession((state) => state.scan !== null);
  const locked = useSession((state) => isRunActive(state.run)) || scanning;

  if (inputs.roots.length === 0) {
    return (
      <section className={styles.panel} aria-labelledby="inputs-heading">
        <h2 id="inputs-heading" className={styles.heading}>
          Inputs
        </h2>
        <div className={styles.dropZone}>
          <p className={styles.dropTitle}>Drop images or folders here</p>
          <p className={styles.dropHint}>Folders keep their structure. Nothing is changed until you convert.</p>
          <div className={styles.actions}>
            <AddButtons disabled={locked} />
          </div>
          <ScanStatus />
        </div>
      </section>
    );
  }

  return (
    <section className={styles.panel} aria-labelledby="inputs-heading">
      <div className={styles.toolbar}>
        <h2 id="inputs-heading" className={styles.heading}>
          Inputs
        </h2>
        <div className={styles.actions}>
          <AddButtons disabled={locked} />
          <button type="button" className="button" disabled={locked} onClick={() => void clearInputs()}>
            Clear
          </button>
        </div>
      </div>
      <ScanStatus />
      <ul className={styles.roots} aria-label="Added files and folders">
        {inputs.roots.map((root) => (
          <li key={root.id} className={styles.root}>
            <span className={styles.kind}>{root.isFolder ? "Folder" : "File"}</span>
            <span className={styles.path} title={root.path}>
              {root.path}
            </span>
            {root.isFolder && <span className={styles.count}>{plural(root.files, "file")}</span>}
            <button
              type="button"
              className={styles.remove}
              aria-label={`Remove ${root.path}`}
              disabled={locked}
              onClick={() => void removeRoot(root.id)}
            >
              ×
            </button>
          </li>
        ))}
      </ul>
      <Breakdown inputs={inputs} />
    </section>
  );
}

import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useState } from "react";
import type { EntryDetails, Job } from "@bindings";
import { errorMessage } from "../../api/client";
import { FORMAT_LABELS, TOOL_LABELS } from "../../lib/formats";
import { formatBytes } from "../../lib/units";
import { isRunActive } from "../../state/session";
import { useApi, useSession } from "../../state/sessionContext";
import styles from "./FileDrawer.module.css";

function colour(details: EntryDetails): string {
  const parts: string[] = [];
  if (details.bitDepth !== null) parts.push(`${details.bitDepth}-bit`);
  if (details.alpha === true) parts.push("has transparency");
  if (details.alpha === false) parts.push("no transparency");
  return parts.length > 0 ? parts.join(", ") : "Not stated in the header";
}

function saving(input: number, output: number): string {
  if (input === 0) return "";
  const change = Math.round((1 - output / input) * 100);
  return change >= 0 ? `${change}% smaller` : `${-change}% larger`;
}

function OutputCard({ job, onReveal }: { job: Job; onReveal(): void }) {
  const { state } = job;
  const tools = job.tools.map((tool) => TOOL_LABELS[tool].split(" ")[0]).join(" then ");
  return (
    <li className={styles.output}>
      <div className={styles.outputTop}>
        <span className={styles.outputName}>{FORMAT_LABELS[job.format]}</span>
        {state.state === "converted" && (
          <span className={styles.num}>
            <span className={styles.ok}>{formatBytes(state.outputBytes)}</span>{" "}
            <span className={styles.muted}>({saving(state.inputBytes, state.outputBytes)})</span>
          </span>
        )}
        {state.state === "failed" && <span className={styles.bad}>Failed</span>}
        {state.state === "running" && <span className={styles.run}>Converting…</span>}
        {state.state === "ready" && <span className={styles.muted}>Waiting</span>}
        {state.state === "cancelled" && <span className={styles.muted}>Cancelled</span>}
        {(state.state === "skipped" || state.state === "blocked") && (
          <span className={styles.warn}>{state.state === "skipped" ? "Skipped" : "Can't convert"}</span>
        )}
      </div>
      <span className={styles.path} title={job.route ?? undefined}>
        {job.output}
      </span>
      {state.state === "converted" && (
        <span className={styles.muted}>
          Made with {tools} in {state.seconds.toFixed(1)} s
        </span>
      )}
      {state.state === "ready" && tools && <span className={styles.muted}>Will be made with {tools}</span>}
      {state.state === "failed" && <p className={styles.reason}>{state.message}</p>}
      {(state.state === "skipped" || state.state === "blocked") && <p className={styles.reason}>{state.reason.text}</p>}
      {job.notes.length > 0 && (
        <ul className={styles.notes}>
          {job.notes.map((note) => (
            <li key={note.code + note.text}>{note.text}</li>
          ))}
        </ul>
      )}
      {state.state === "converted" && (
        <div className={styles.actions}>
          <button type="button" className="button" onClick={onReveal}>
            Show in folder
          </button>
        </div>
      )}
    </li>
  );
}

function Details({ details }: { details: EntryDetails }) {
  const reveal = useSession((state) => state.reveal);
  const setExcluded = useSession((state) => state.setExcluded);
  const locked = useSession((state) => isRunActive(state.run));
  const { file } = details;
  const image = file.format !== null;

  return (
    <>
      <section className={styles.section}>
        <h3 className={styles.label}>Original</h3>
        <dl className={styles.facts}>
          <dt>Format</dt>
          <dd>
            {file.description}
            {image && <span className={styles.muted}> (recognised from its content)</span>}
            {details.extensionMismatch && <span className={styles.warn}>; its extension says otherwise</span>}
          </dd>
          {file.width !== null && file.height !== null && (
            <>
              <dt>Dimensions</dt>
              <dd className={styles.num}>
                {file.width} × {file.height}
              </dd>
            </>
          )}
          <dt>Size</dt>
          <dd className={styles.num}>{formatBytes(file.size)}</dd>
          {image && (
            <>
              <dt>Colour</dt>
              <dd>{colour(details)}</dd>
              <dt>Metadata</dt>
              <dd>{details.metadata.length > 0 ? details.metadata.join(", ") : "None found"}</dd>
            </>
          )}
          {details.frames !== null && (
            <>
              <dt>Frames</dt>
              <dd>{details.frames}</dd>
            </>
          )}
          {details.pages !== null && details.pages > 1 && (
            <>
              <dt>Pages</dt>
              <dd>{details.pages} (the first is converted)</dd>
            </>
          )}
        </dl>
        {file.problem && <p className={styles.reason}>The file looks damaged: {file.problem}</p>}
        <div className={styles.actions}>
          <button type="button" className="button" onClick={() => void reveal({ kind: "source", entry: file.entry })}>
            Show original in folder
          </button>
        </div>
      </section>

      {image && (
        <section className={styles.section}>
          <h3 className={styles.label}>Outputs</h3>
          {details.jobs.length > 0 ? (
            <ul className={styles.outputs}>
              {details.jobs.map((job) => (
                <OutputCard key={job.id} job={job} onReveal={() => void reveal({ kind: "output", job: job.id })} />
              ))}
            </ul>
          ) : (
            <p className={styles.muted}>
              {file.excluded ? "Excluded, so nothing will be made from it." : "Pick an output to see what this file becomes."}
            </p>
          )}
        </section>
      )}

      {image && (
        <section className={styles.section}>
          <label className={styles.include}>
            <input
              type="checkbox"
              checked={!file.excluded}
              disabled={locked}
              onChange={(event) => void setExcluded(details.root, details.relativePath, !event.target.checked)}
            />
            Include this file in the next conversion
          </label>
        </section>
      )}
    </>
  );
}

/** Everything about the selected file, over the outputs column. */
export function FileDrawer() {
  const api = useApi();
  const selected = useSession((state) => state.selected);
  const version = useSession((state) => state.listVersion);
  const select = useSession((state) => state.select);
  const [details, setDetails] = useState<EntryDetails | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (selected === null) return;
    let current = true;
    api.entry(selected).then(
      (loaded) => {
        if (!current) return;
        setDetails(loaded);
        setError(null);
      },
      (failure: unknown) => {
        if (current) setError(errorMessage(failure));
      },
    );
    return () => {
      current = false;
    };
  }, [api, selected, version]);

  const shown = details?.file.entry === selected ? details : null;
  return (
    <Dialog.Root open={selected !== null} onOpenChange={(open) => !open && select(null)}>
      <Dialog.Portal>
        <Dialog.Overlay className={styles.overlay} />
        <Dialog.Content className={styles.drawer} aria-describedby={undefined}>
          <header className={styles.header}>
            <div className={styles.headerText}>
              <Dialog.Title className={styles.title}>{shown?.name ?? "File"}</Dialog.Title>
              {shown && <p className={styles.fullPath}>{shown.path}</p>}
            </div>
            <Dialog.Close asChild>
              <button type="button" className={`button ${styles.close}`} aria-label="Close">
                ×
              </button>
            </Dialog.Close>
          </header>
          <div className={styles.body}>
            {error && <p className={styles.reason}>{error}</p>}
            {shown ? <Details details={shown} /> : !error && <p className={styles.muted}>Loading…</p>}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

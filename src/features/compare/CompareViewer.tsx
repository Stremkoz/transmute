import * as Dialog from "@radix-ui/react-dialog";
import { useEffect, useState } from "react";
import type { EntryDetails, Job, LosslessChoice, OutputTarget, Preview, Sample } from "@bindings";
import { errorMessage, isApiError } from "../../api/client";
import { chosenSettings, defaultsFor, effective, losslessChoiceText, settingsSummary } from "../../lib/codecs";
import { FORMAT_LABELS } from "../../lib/formats";
import { formatBytes, formatCount } from "../../lib/units";
import type { ViewerState } from "../../state/session";
import { useApi, useSession } from "../../state/sessionContext";
import styles from "./CompareViewer.module.css";
import type { Layout, Pan, Zoom } from "./geometry";
import { ImageStage } from "./ImageStage";
import { useViewerFiles } from "./useViewerFiles";

/** A promise's result, kept with the key it was asked for, so stale answers never show. */
type Loaded<T> = { key: string } & ({ value: T } | { error: string; gone: boolean });

function useLoad<T>(key: string | null, load: () => Promise<T>): Loaded<T> | null {
  const [result, setResult] = useState<Loaded<T> | null>(null);
  useEffect(() => {
    if (key === null) return;
    let current = true;
    load().then(
      (value) => current && setResult({ key, value }),
      (error: unknown) =>
        current && setResult({ key, error: errorMessage(error), gone: isApiError(error) && error.code === "unavailable" }),
    );
    return () => {
      current = false;
    };
    // `load` is a fresh closure each render; `key` says when it asks for something new.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return result?.key === key ? result : null;
}

/** "87% smaller" in green, or "172% larger" in amber: a bigger file isn't a saving. */
function Change({ input, output }: { input: number; output: number }) {
  if (input === 0) return null;
  const percent = Math.round((1 - output / input) * 100);
  return percent >= 0 ? (
    <span className={styles.saving}>({percent}% smaller)</span>
  ) : (
    <span className={styles.larger}>({-percent}% larger)</span>
  );
}

function dims(width: number | null, height: number | null): string {
  return width !== null && height !== null ? `${width} × ${height}` : "—";
}

/** What can be put beside the original: converted outputs, or every output as a trial. */
interface Choice {
  target: number;
  label: string;
  job?: Job;
}

function choicesFor(mode: ViewerState["mode"], details: EntryDetails, outputs: OutputTarget[]): Choice[] {
  if (mode === "trial") {
    return outputs.map((output, target) => ({ target, label: FORMAT_LABELS[output.codec.format] }));
  }
  return details.jobs
    .filter((job) => job.state.state === "converted")
    .map((job) => ({
      target: job.target,
      job,
      label: `${FORMAT_LABELS[job.format]}${job.state.state === "converted" ? ` · ${formatBytes(job.state.outputBytes)}` : ""}`,
    }));
}

interface OutputSide {
  preview: Preview;
  bytes: number;
  seconds: number | null;
  notes: string[];
  /** What Auto chose, when it encoded both ways. */
  choice: LosslessChoice | null;
}

function Viewer({ viewer }: { viewer: ViewerState }) {
  const api = useApi();
  const preset = useSession((state) => state.overview?.preset);
  const codecs = useSession((state) => state.codecs);
  const filter = useSession((state) => state.filter);
  const listVersion = useSession((state) => state.listVersion);
  const openViewer = useSession((state) => state.openViewer);
  const [layout, setLayout] = useState<Layout>("divider");
  const [zoom, setZoom] = useState<Zoom>("fit");
  const [pan, setPan] = useState<Pan>({ x: 0, y: 0 });
  const [peeking, setPeeking] = useState(false);
  const { mode, entry } = viewer;

  const files = useViewerFiles(api, mode, filter, listVersion);
  const position = files?.indexOf(entry) ?? -1;
  const details = useLoad(`entry|${entry}|${listVersion}`, () => api.entry(entry));
  const source = useLoad(`source|${entry}`, () => api.preview({ kind: "source", entry }));

  const outputs = preset?.outputs ?? [];
  const choices = details && "value" in details ? choicesFor(mode, details.value, outputs) : [];
  const choice = choices.find((c) => c.target === viewer.target) ?? choices[0];
  const output = choice ? outputs[choice.target] : undefined;
  const outputKey = !choice
    ? null
    : mode === "compare"
      ? `output|${choice.job?.id}|${listVersion}`
      : // A trial depends on the settings: edit them and it runs again.
        `trial|${entry}|${choice.target}|${JSON.stringify(output)}`;
  const side = useLoad<OutputSide>(outputKey, async () => {
    if (!choice) throw new Error("nothing to compare");
    if (mode === "trial") {
      const sample: Sample = await api.sample(entry, choice.target);
      return {
        preview: sample.preview,
        bytes: sample.outputBytes,
        seconds: sample.seconds,
        notes: sample.notes.map((n) => n.text),
        choice: sample.losslessChoice ?? null,
      };
    }
    const job = choice.job;
    if (!job || job.state.state !== "converted") throw new Error("not converted");
    const preview = await api.preview({ kind: "output", job: job.id });
    return {
      preview,
      bytes: job.state.outputBytes,
      seconds: job.state.seconds,
      notes: job.notes.map((n) => n.text),
      choice: job.state.losslessChoice ?? null,
    };
  });

  const go = (step: number) => {
    if (!files || files.length === 0) return;
    const next = files[(Math.max(position, 0) + step + files.length) % files.length];
    if (next !== undefined && next !== entry) {
      setPan({ x: 0, y: 0 });
      openViewer({ ...viewer, entry: next });
    }
  };

  // ← → files · 0 fit · 1 100% · 2 200% · hold Space for the original.
  useEffect(() => {
    const down = (event: KeyboardEvent) => {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
      if (event.key === "ArrowLeft") go(-1);
      else if (event.key === "ArrowRight") go(1);
      else if (event.key === "0") setZoom("fit");
      else if (event.key === "1") setZoom(1);
      else if (event.key === "2") setZoom(2);
      else if (event.key === " ") setPeeking(true);
      else return;
      event.preventDefault();
    };
    const up = (event: KeyboardEvent) => event.key === " " && setPeeking(false);
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  });

  const info = details && "value" in details ? details.value : null;
  const original = source && "value" in source ? source.value : null;
  const result = side && "value" in side ? side.value : null;
  const image = {
    width: original?.width ?? info?.file.width ?? 1600,
    height: original?.height ?? info?.file.height ?? 900,
  };
  const outputName = output ? FORMAT_LABELS[output.codec.format] : "Output";
  const trialTag = mode === "trial" ? " (trial)" : "";
  const settings = (() => {
    const codec = output && defaultsFor(codecs, output.codec.format);
    return codec && output ? settingsSummary(chosenSettings(effective(codec.inherited, output.codec), result?.choice)) : "";
  })();

  const outputMessage = (() => {
    if (!choice) {
      return mode === "trial" ? "Add an output to try it on this file." : "This file has no converted output to compare.";
    }
    if (side === null) return mode === "trial" ? `Converting a trial ${outputName}…` : "Loading…";
    if ("error" in side) {
      return side.gone ? `The ${outputName} file is no longer there (moved or deleted?).` : side.error;
    }
    return null;
  })();

  const folder = info?.relativePath.includes("/") ? info.relativePath.slice(0, info.relativePath.lastIndexOf("/")) : "";

  return (
    <>
      <header className={styles.header}>
        <div className={styles.titleBlock}>
          <Dialog.Title className={styles.title}>{info?.name ?? "…"}</Dialog.Title>
          <Dialog.Description className={styles.subtitle}>
            {[folder.replaceAll("/", "\\"), files && position >= 0 ? `file ${formatCount(position + 1)} of ${formatCount(files.length)}` : ""]
              .filter(Boolean)
              .join(" · ")}
            {mode === "trial" && " · trial: converted into a cache, nothing is written beside your files"}
          </Dialog.Description>
        </div>
        <div className={styles.controls}>
          <div className={styles.nav}>
            <button type="button" className="button" aria-label="Previous file" disabled={!files || files.length < 2} onClick={() => go(-1)}>
              ‹
            </button>
            <button type="button" className="button" aria-label="Next file" disabled={!files || files.length < 2} onClick={() => go(1)}>
              ›
            </button>
          </div>
          {choices.length > 0 && (
            <div className={styles.group} role="group" aria-label="Compare with">
              <span className={styles.groupLabel}>Compare with</span>
              {choices.map((c) => (
                <button
                  key={c.target}
                  type="button"
                  aria-pressed={c === choice}
                  onClick={() => openViewer({ ...viewer, target: c.target })}
                >
                  {c.label}
                </button>
              ))}
            </div>
          )}
          <div className={styles.group} role="group" aria-label="Layout">
            {(["divider", "side"] as const).map((value) => (
              <button key={value} type="button" aria-pressed={layout === value} onClick={() => setLayout(value)}>
                {value === "divider" ? "Divider" : "Side by side"}
              </button>
            ))}
          </div>
          <div className={styles.group} role="group" aria-label="Zoom">
            {(["fit", 1, 2] as const).map((value) => (
              <button
                key={value}
                type="button"
                aria-pressed={zoom === value}
                onClick={() => {
                  setZoom(value);
                  setPan({ x: 0, y: 0 });
                }}
              >
                {value === "fit" ? "Fit" : `${value * 100}%`}
              </button>
            ))}
          </div>
          <Dialog.Close asChild>
            <button type="button" className={`button ${styles.close}`} aria-label="Close">
              ×
            </button>
          </Dialog.Close>
        </div>
      </header>

      <main className={styles.body}>
        {source && "error" in source ? (
          <div className={styles.message}>
            {source.gone ? "The original is no longer there (moved or deleted?)." : source.error}
          </div>
        ) : original ? (
          <ImageStage
            original={api.previewUrl(original)}
            output={result ? { url: api.previewUrl(result.preview) } : { message: outputMessage }}
            image={image}
            layout={layout}
            zoom={zoom}
            pan={pan}
            onPan={setPan}
            peeking={peeking}
            originalLabel={`Original · ${info ? `${info.file.description} · ${formatBytes(info.file.size)}` : ""}`}
            outputLabel={
              result ? (
                <>
                  {outputName}
                  {trialTag} · {formatBytes(result.bytes)}{" "}
                  <Change input={info?.file.size ?? 0} output={result.bytes} />
                </>
              ) : (
                `${outputName}${trialTag}`
              )
            }
          />
        ) : (
          <div className={styles.message}>Loading…</div>
        )}
      </main>

      <footer className={styles.footer}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th scope="col" />
              <th scope="col">Original</th>
              <th scope="col">
                {outputName}
                {trialTag}
              </th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th scope="row">Format</th>
              <td>{info?.file.description ?? "—"}</td>
              <td>
                {output ? `${outputName}, ${settings.charAt(0).toLowerCase()}${settings.slice(1)}` : "—"}
                {result?.choice && <span className={styles.muted}> · {losslessChoiceText(result.choice)}</span>}
              </td>
            </tr>
            <tr>
              <th scope="row">Dimensions</th>
              <td>{dims(info?.file.width ?? null, info?.file.height ?? null)}</td>
              <td>{result ? dims(result.preview.width, result.preview.height) : "—"}</td>
            </tr>
            <tr>
              <th scope="row">File size</th>
              <td>{info ? formatBytes(info.file.size) : "—"}</td>
              <td>
                {result ? (
                  <>
                    {formatBytes(result.bytes)} <Change input={info?.file.size ?? 0} output={result.bytes} />
                    {result.seconds !== null && <span className={styles.muted}> · made in {result.seconds.toFixed(1)} s</span>}
                  </>
                ) : (
                  "—"
                )}
              </td>
            </tr>
            <tr>
              <th scope="row">Metadata</th>
              <td>{info ? info.metadata.join(", ") || "None" : "—"}</td>
              <td>{result ? result.preview.metadata.join(", ") || "None" : "—"}</td>
            </tr>
          </tbody>
        </table>
        <div className={styles.footerNotes}>
          {result && result.notes.length > 0 && (
            <ul className={styles.notes}>
              {result.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          )}
          <p className={styles.keys}>
            Hold <kbd>Space</kbd> to see the original · <kbd>←</kbd> <kbd>→</kbd> previous / next file · <kbd>0</kbd> fit ·{" "}
            <kbd>1</kbd> 100% · <kbd>2</kbd> 200%
            {zoom !== "fit" && " · drag to pan, both sides move together"}
          </p>
        </div>
      </footer>
    </>
  );
}

/** The full-window compare viewer (mockup screen 7), also used for trial conversions. */
export function CompareViewer() {
  const viewer = useSession((state) => state.viewer);
  const closeViewer = useSession((state) => state.closeViewer);
  return (
    <Dialog.Root open={viewer !== null} onOpenChange={(open) => !open && closeViewer()}>
      <Dialog.Portal>
        <Dialog.Content className={styles.viewer}>
          {viewer && <Viewer viewer={viewer} />}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

import type { FileCounts, FileInfo, FileOutput } from "@bindings";
import { FORMAT_LABELS } from "../../lib/formats";
import { formatCount, plural } from "../../lib/units";
import styles from "./FilesList.module.css";

/** Why a file's outputs didn't (or won't) get written, for filtered views. */
export function failureReasons(outputs: FileOutput[]): string[] {
  return outputs.flatMap((output) => {
    const name = FORMAT_LABELS[output.format];
    const { state } = output;
    if (state.state === "failed") return [`${name}: ${state.message}`];
    if (state.state === "skipped" || state.state === "blocked") return [`${name}: ${state.reason.text}`];
    return [];
  });
}

/** The status text of a folder, from its files' counts. */
export function folderStatus(counts: FileCounts): Array<{ text: string; tone: string }> {
  const parts: Array<{ text: string; tone: string }> = [];
  if (counts.running > 0) parts.push({ text: `${formatCount(counts.running)} converting`, tone: styles.runText ?? "" });
  if (counts.failed > 0) parts.push({ text: `${formatCount(counts.failed)} failed`, tone: styles.failText ?? "" });
  if (counts.done > 0) parts.push({ text: `${formatCount(counts.done)} done`, tone: styles.okText ?? "" });
  if (counts.ready > 0) parts.push({ text: `${formatCount(counts.ready)} to convert`, tone: "" });
  if (counts.problems > 0) parts.push({ text: `${formatCount(counts.problems)} skipped`, tone: styles.warnText ?? "" });
  if (counts.cancelled > 0) parts.push({ text: `${formatCount(counts.cancelled)} cancelled`, tone: "" });
  return parts.slice(0, 3);
}

/** Images below a folder, for its name column. */
export function folderSize(counts: FileCounts): string {
  return plural(counts.all - counts.unsupported, "image");
}

/** A folder's include state from its counts: all, none or some. */
export function folderInclusion(counts: FileCounts): { checked: boolean; mixed: boolean } {
  const images = counts.all - counts.unsupported;
  return { checked: counts.excluded < images, mixed: counts.excluded > 0 && counts.excluded < images };
}

export function dimensions(file: FileInfo): string {
  return file.width !== null && file.height !== null ? `${file.width} × ${file.height}` : "";
}

/** Props that make a row open on click, Enter or Space. */
export function activates(open: () => void) {
  return {
    tabIndex: 0,
    onClick: open,
    onKeyDown: (event: { key: string; preventDefault(): void }) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        open();
      }
    },
  };
}

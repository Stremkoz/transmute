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

/** Row heights: plain, and with thumbnails (a 48 px preview plus padding). */
export const ROW_HEIGHT = 34;
export const THUMB_ROW_HEIGHT = 58;

/** True when a file has an output the compare viewer can show. */
export function hasConverted(file: FileInfo): boolean {
  return file.outputs.some((output) => output.state.state === "converted");
}

export function dimensions(file: FileInfo): string {
  return file.width !== null && file.height !== null ? `${file.width} × ${file.height}` : "";
}

/** How long a click waits to see whether it's the first of a double-click. */
export const DOUBLE_CLICK_MS = 250;

// At most one click is waiting at a time, so one timer is enough.
let pendingClick: ReturnType<typeof setTimeout> | undefined;

/**
 * Props that make a row open on click, Enter or Space, and `compare` on
 * double-click. A row that can be compared opens a moment after the click:
 * opened at once, the drawer's overlay would catch the second click.
 */
export function activates(open: () => void, compare?: () => void) {
  return {
    tabIndex: 0,
    onClick: (event: { detail: number }) => {
      clearTimeout(pendingClick);
      if (!compare) open();
      else if (event.detail < 2) pendingClick = setTimeout(open, DOUBLE_CLICK_MS);
    },
    onDoubleClick: compare
      ? () => {
          clearTimeout(pendingClick);
          compare();
        }
      : undefined,
    onKeyDown: (event: { key: string; preventDefault(): void }) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        open();
      }
    },
  };
}

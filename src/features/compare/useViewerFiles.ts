import { useEffect, useState } from "react";
import type { FileFilter, FileRow } from "@bindings";
import type { TransmuteApi } from "../../api/client";
import type { ViewerState } from "../../state/session";

const PAGE = 1000;

/**
 * The tree's order: by input, then at each level folders before files,
 * each by name. The unfiltered list is a tree, so ← / → follow it.
 */
export function treeOrder(a: FileRow, b: FileRow): number {
  if (a.root !== b.root) return a.root - b.root;
  const left = a.relativePath.split("/");
  const right = b.relativePath.split("/");
  for (let i = 0; i < Math.min(left.length, right.length); i++) {
    if (left[i] === right[i]) continue;
    const leftFolder = i < left.length - 1;
    const rightFolder = i < right.length - 1;
    if (leftFolder !== rightFolder) return leftFolder ? -1 : 1;
    return (left[i] ?? "").localeCompare(right[i] ?? "");
  }
  return left.length - right.length;
}

/** Which files the viewer steps through in each mode. */
export function belongs(mode: ViewerState["mode"], row: FileRow): boolean {
  const { file } = row;
  if (mode === "compare") return file.outputs.some((output) => output.state.state === "converted");
  return file.format !== null && !file.excluded;
}

/**
 * The files ← / → step through: the list's own order and filter (tree
 * order when unfiltered), keeping
 * those with a converted output (compare) or any included image (trial).
 * `null` while loading.
 */
export function useViewerFiles(api: TransmuteApi, mode: ViewerState["mode"] | null, filter: FileFilter, version: number) {
  const [files, setFiles] = useState<{ key: string; ids: number[] } | null>(null);
  const key = `${mode}|${filter}|${version}`;

  useEffect(() => {
    if (mode === null) return;
    let current = true;
    const load = async () => {
      const rows: FileRow[] = [];
      for (let offset = 0; ; offset += PAGE) {
        const page = await api.files(filter, offset, PAGE);
        rows.push(...page.files.filter((row) => belongs(mode, row)));
        if (offset + PAGE >= page.total) break;
      }
      if (filter === "all") rows.sort(treeOrder);
      if (current) setFiles({ key, ids: rows.map((row) => row.file.entry) });
    };
    void load().catch(() => current && setFiles({ key, ids: [] }));
    return () => {
      current = false;
    };
  }, [api, mode, filter, key]);

  return files?.key === key ? files.ids : null;
}

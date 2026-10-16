import { useEffect, useState } from "react";
import type { FileFilter, FileRow } from "@bindings";
import { FORMAT_LABELS } from "../../lib/formats";
import { formatBytes, formatCount } from "../../lib/units";
import { useWindowedRows } from "../../lib/virtual";
import { useApi, useSession } from "../../state/sessionContext";
import styles from "./FilesList.module.css";
import { OutputChips } from "./rows";
import { ROW_HEIGHT, THUMB_ROW_HEIGHT, activates, dimensions, failureReasons, hasConverted } from "./rowHelpers";
import { Thumbnail } from "./Thumbnail";
import { ListHead } from "./TreeList";

const PAGE = 200;
/** Filters whose rows carry a reason line. */
const WITH_REASONS = new Set<FileFilter>(["failed", "problems", "cancelled"]);

/**
 * A filtered files list: every matching file across the inputs, flat,
 * with its folder and, for problems, why. Pages load as they scroll in.
 */
export function FlatList({ filter }: { filter: FileFilter }) {
  const api = useApi();
  const version = useSession((state) => state.listVersion);
  const selected = useSession((state) => state.selected);
  const select = useSession((state) => state.select);
  const setFilter = useSession((state) => state.setFilter);
  const [total, setTotal] = useState(0);
  const [pages, setPages] = useState<Map<number, FileRow[]>>(() => new Map());
  const openViewer = useSession((state) => state.openViewer);
  const thumbnails = useSession((state) => state.thumbnails);
  const tall = WITH_REASONS.has(filter);
  const rowHeight = thumbnails ? THUMB_ROW_HEIGHT : tall ? 52 : ROW_HEIGHT;
  const { ref, onScroll, start, end, top, height } = useWindowedRows(total, rowHeight);
  const firstPage = Math.floor(start / PAGE);
  const lastPage = Math.floor(Math.max(end - 1, 0) / PAGE);

  // A new filter or new data: start again from the pages in view.
  useEffect(() => {
    let current = true;
    const wanted = Array.from({ length: lastPage - firstPage + 1 }, (_, i) => firstPage + i);
    void Promise.all(wanted.map((page) => api.files(filter, page * PAGE, PAGE))).then((loaded) => {
      if (!current) return;
      setTotal(loaded[0]?.total ?? 0);
      setPages(new Map(loaded.map((page, i) => [wanted[i] ?? 0, page.files])));
    });
    return () => {
      current = false;
    };
  }, [api, filter, version, firstPage, lastPage]);

  const rows: Array<FileRow | null> = [];
  for (let index = start; index < end; index++) {
    rows.push(pages.get(Math.floor(index / PAGE))?.[index % PAGE] ?? null);
  }

  return (
    <div
      className={`${styles.scroller} ${thumbnails ? styles.withThumbs : ""}`}
      ref={ref}
      onScroll={onScroll}
      role="grid"
      aria-label="Files"
      aria-rowcount={total}
    >
      <ListHead />
      <div style={{ height: height, position: "relative" }}>
        <div style={{ position: "absolute", top: top, left: 0, right: 0 }}>
          {rows.map((row, i) => {
            const index = start + i;
            if (!row) {
              return <div key={`pending-${index}`} className={`${styles.row} ${tall ? styles.tall : ""}`} />;
            }
            const { file } = row;
            const folder = row.relativePath.includes("/") ? row.relativePath.slice(0, row.relativePath.lastIndexOf("/") + 1) : "";
            const reasons = failureReasons(file.outputs);
            const isSelected = selected === file.entry;
            return (
              <div
                key={file.entry}
                className={`${styles.row} ${styles.clickable} ${tall ? styles.tall : ""} ${isSelected ? styles.selected : ""}`}
                role="row"
                aria-label={row.relativePath}
                aria-selected={isSelected}
                {...activates(
                  () => select(file.entry),
                  hasConverted(file) ? () => openViewer({ mode: "compare", entry: file.entry, target: null }) : undefined,
                )}
              >
                <span />
                <div className={styles.thumbRow}>
                  {thumbnails && (file.format !== null ? <Thumbnail entry={file.entry} /> : <span className={styles.thumb} />)}
                  <div className={styles.nameStack}>
                    <div className={styles.name}>
                      <span>
                        <span className={styles.meta}>{folder.replaceAll("/", "\\")}</span>
                        {row.name}
                      </span>
                    </div>
                    {tall && reasons.length > 0 && (
                      <span className={styles.reason} title={reasons.join("\n")}>
                        {reasons.join(" · ")}
                      </span>
                    )}
                  </div>
                </div>
                <span className={styles.muted}>{file.format ? FORMAT_LABELS[file.format] : ""}</span>
                <span className={`${styles.muted} ${styles.num}`}>{dimensions(file)}</span>
                <span className={`${styles.num} ${styles.size}`}>{formatBytes(file.size)}</span>
                <div className={styles.outs}>
                  {file.excluded ? <span>Excluded</span> : <OutputChips outputs={file.outputs} />}
                </div>
              </div>
            );
          })}
        </div>
      </div>
      <p className={styles.empty}>
        {total === 0 ? "No files match. " : `Showing ${formatCount(total)} of the files. `}
        <button type="button" className="link" onClick={() => setFilter("all")}>
          Show all
        </button>
      </p>
    </div>
  );
}

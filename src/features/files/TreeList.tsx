import type { ReactNode } from "react";
import { formatBytes, plural } from "../../lib/units";
import { useWindowedRows } from "../../lib/virtual";
import { isRunActive } from "../../state/session";
import { useSession } from "../../state/sessionContext";
import styles from "./FilesList.module.css";
import { FORMAT_LABELS } from "../../lib/formats";
import { IncludeBox, OutputChips } from "./rows";
import {
  ROW_HEIGHT,
  THUMB_ROW_HEIGHT,
  activates,
  dimensions,
  folderInclusion,
  folderSize,
  folderStatus,
  hasConverted,
} from "./rowHelpers";
import { Thumbnail } from "./Thumbnail";
import { useFileTree, type TreeRow } from "./useFileTree";

const INDENT = 20;

function Chevron({ open }: { open: boolean }) {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
      <path d={open ? "m6 9 6 6 6-6" : "m9 6 6 6-6 6"} />
    </svg>
  );
}

export function ListHead() {
  return (
    <div className={styles.head} role="row">
      <span role="columnheader" aria-label="Include" />
      <span role="columnheader">Name</span>
      <span role="columnheader">Format</span>
      <span role="columnheader">Dimensions</span>
      <span role="columnheader" className={styles.size}>
        Size
      </span>
      <span role="columnheader">Outputs</span>
    </div>
  );
}

function Status({ parts }: { parts: Array<{ text: string; tone: string }> }) {
  return (
    <div className={styles.outs}>
      {parts.map((part) => (
        <span key={part.text} className={part.tone}>
          {part.text}
        </span>
      ))}
    </div>
  );
}

/** The unfiltered files list: inputs, folders (opened on demand) and files. */
export function TreeList() {
  const { rows, toggle } = useFileTree();
  const locked = useSession((state) => isRunActive(state.run));
  const selected = useSession((state) => state.selected);
  const select = useSession((state) => state.select);
  const setExcluded = useSession((state) => state.setExcluded);
  const removeRoot = useSession((state) => state.removeRoot);
  const openViewer = useSession((state) => state.openViewer);
  const thumbnails = useSession((state) => state.thumbnails);
  const { ref, onScroll, start, end, top, height } = useWindowedRows(
    rows.length,
    thumbnails ? THUMB_ROW_HEIGHT : ROW_HEIGHT,
  );

  const draw = (row: TreeRow): ReactNode => {
    switch (row.kind) {
      case "loading":
        return (
          <div key={row.key} className={`${styles.row} ${styles.dim}`} role="row">
            <span />
            <span style={{ paddingLeft: row.depth * INDENT }}>Loading…</span>
          </div>
        );
      case "root": {
        const { root } = row;
        const name = root.path.split(/[\\/]/).filter(Boolean).pop() ?? root.path;
        const inclusion = folderInclusion(root.counts);
        return (
          <div key={row.key} className={styles.row} role="row" aria-label={name}>
            <IncludeBox
              label={`Include ${name}`}
              {...inclusion}
              disabled={locked}
              onChange={(include) => void setExcluded(root.id, "", !include)}
            />
            <div className={styles.name} title={root.path}>
              <button type="button" className={styles.twisty} aria-label={row.open ? `Close ${name}` : `Open ${name}`} aria-expanded={row.open} onClick={() => toggle(row.key)}>
                <Chevron open={row.open} />
              </button>
              <span className={styles.folderName}>{name}</span>
              <span className={styles.meta}>{folderSize(root.counts)}</span>
              <button
                type="button"
                className={styles.remove}
                aria-label={`Remove ${root.path}`}
                disabled={locked}
                onClick={() => void removeRoot(root.id)}
              >
                ×
              </button>
            </div>
            <span />
            <span />
            <span />
            <Status parts={folderStatus(root.counts)} />
          </div>
        );
      }
      case "folder": {
        const inclusion = folderInclusion(row.counts);
        return (
          <div key={row.key} className={styles.row} role="row" aria-label={row.relativePath}>
            <IncludeBox
              label={`Include ${row.relativePath}`}
              {...inclusion}
              disabled={locked}
              onChange={(include) => void setExcluded(row.root, row.relativePath, !include)}
            />
            <div className={styles.name} style={{ paddingLeft: row.depth * INDENT }}>
              <button type="button" className={styles.twisty} aria-label={row.open ? `Close ${row.name}` : `Open ${row.name}`} aria-expanded={row.open} onClick={() => toggle(row.key)}>
                <Chevron open={row.open} />
              </button>
              <span className={styles.folderName}>{row.name}</span>
              <span className={styles.meta}>{folderSize(row.counts)}</span>
            </div>
            <span />
            <span />
            <span />
            <Status parts={folderStatus(row.counts)} />
          </div>
        );
      }
      case "file": {
        const { file } = row;
        const image = file.format !== null;
        const tone = file.excluded || !image ? styles.dim : "";
        const isSelected = selected === file.entry;
        return (
          <div
            key={row.key}
            className={`${styles.row} ${styles.clickable} ${tone} ${isSelected ? styles.selected : ""}`}
            role="row"
            aria-label={row.relativePath}
            aria-selected={isSelected}
            {...activates(
              () => select(file.entry),
              hasConverted(file) ? () => openViewer({ mode: "compare", entry: file.entry, target: null }) : undefined,
            )}
          >
            {image ? (
              <IncludeBox
                label={`Include ${row.relativePath}`}
                checked={!file.excluded}
                disabled={locked}
                onChange={(include) => void setExcluded(row.root, row.relativePath, !include)}
              />
            ) : (
              <span />
            )}
            <div className={styles.name} style={{ paddingLeft: row.depth * INDENT + 24 }}>
              {thumbnails && (image ? <Thumbnail entry={file.entry} /> : <span className={styles.thumb} />)}
              <span>{row.name}</span>
            </div>
            <span className={styles.muted}>{file.format ? FORMAT_LABELS[file.format] : ""}</span>
            <span className={`${styles.muted} ${styles.num}`}>{dimensions(file)}</span>
            <span className={`${styles.num} ${styles.size}`}>{formatBytes(file.size)}</span>
            <div className={styles.outs}>
              {file.excluded ? (
                <span>Excluded</span>
              ) : image ? (
                <OutputChips outputs={file.outputs} />
              ) : (
                <span>{file.description}: left alone</span>
              )}
            </div>
          </div>
        );
      }
    }
  };

  return (
    <div
      className={`${styles.scroller} ${thumbnails ? styles.withThumbs : ""}`}
      ref={ref}
      onScroll={onScroll}
      role="grid"
      aria-label="Files"
      aria-rowcount={rows.length}
    >
      <ListHead />
      <div style={{ height: height, position: "relative" }}>
        <div style={{ position: "absolute", top: top, left: 0, right: 0 }}>
          {rows.slice(start, end).map(draw)}
        </div>
      </div>
      {rows.length === 0 && <p className={styles.empty}>{plural(0, "file")}</p>}
    </div>
  );
}

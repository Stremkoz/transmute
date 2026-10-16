import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import type { FileCounts, FileFilter, InputsSummary } from "@bindings";
import { formatCount, plural } from "../../lib/units";
import { isRunActive } from "../../state/session";
import { usePlatform, useSession } from "../../state/sessionContext";
import { FlatList } from "./FlatList";
import styles from "./FilesPanel.module.css";
import { TreeList } from "./TreeList";

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

function ClearMenu({ disabled }: { disabled: boolean }) {
  const done = useSession((state) => state.counts.done);
  const clearFinished = useSession((state) => state.clearFinished);
  const clearInputs = useSession((state) => state.clearInputs);
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button type="button" className="button" disabled={disabled}>
          Clear
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
            <path d="m6 9 6 6 6-6" />
          </svg>
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className={styles.menu} align="end" sideOffset={4}>
          <DropdownMenu.Item className={styles.menuItem} disabled={done === 0} onSelect={() => void clearFinished()}>
            <span>
              Clear finished files {done > 0 && <span className={styles.faint}>({formatCount(done)})</span>}
            </span>
            <span className={styles.menuNote}>
              {done > 0 ? "Keeps anything that failed or is still to do" : "Nothing has finished yet"}
            </span>
          </DropdownMenu.Item>
          <DropdownMenu.Item className={styles.menuItem} onSelect={() => void clearInputs()}>
            <span>Clear everything</span>
            <span className={styles.menuNote}>Start over with an empty list; outputs stay picked</span>
          </DropdownMenu.Item>
          <p className={styles.menuFoot}>Only the list is cleared. No files are moved or deleted.</p>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

interface Chip {
  filter: FileFilter;
  label: string;
  tone?: string;
}

/** Before a run: what will happen. During and after: what did. */
const PREPARE_CHIPS: Chip[] = [
  { filter: "ready", label: "Ready" },
  { filter: "problems", label: "Problems", tone: "warn" },
  { filter: "excluded", label: "Excluded" },
  { filter: "unsupported", label: "Not images" },
];
const RUN_CHIPS: Chip[] = [
  { filter: "running", label: "Converting", tone: "run" },
  { filter: "done", label: "Done", tone: "ok" },
  { filter: "failed", label: "Failed", tone: "fail" },
  { filter: "problems", label: "Skipped" },
  { filter: "ready", label: "Waiting" },
  { filter: "cancelled", label: "Cancelled" },
  { filter: "excluded", label: "Excluded" },
];

function count(counts: FileCounts, filter: FileFilter): number {
  return filter === "all" ? counts.all : counts[filter];
}

function FilterChips() {
  const counts = useSession((state) => state.counts);
  const filter = useSession((state) => state.filter);
  const setFilter = useSession((state) => state.setFilter);
  const ran = useSession((state) => state.run !== null);
  // Chips with nothing in them are hidden, except the one in use.
  const chips = (ran ? RUN_CHIPS : PREPARE_CHIPS).filter((chip) => count(counts, chip.filter) > 0 || chip.filter === filter);
  return (
    <div className={styles.chips} role="group" aria-label="Show">
      {[{ filter: "all" as const, label: "All" }, ...chips].map((chip: Chip) => (
        <button
          key={chip.filter}
          type="button"
          className={`${styles.chip} ${chip.filter === filter ? styles.on : ""}`}
          aria-pressed={chip.filter === filter}
          onClick={() => setFilter(chip.filter)}
        >
          <span className={chip.tone ? styles[chip.tone] : undefined}>{chip.label}</span>
          <b>{formatCount(count(counts, chip.filter))}</b>
        </button>
      ))}
    </div>
  );
}

function summaryLine(inputs: InputsSummary): string {
  const parts = [`${plural(inputs.roots.length, "input")} added`, plural(inputs.convertible, "image")];
  if (inputs.unsupported > 0) parts.push(`${plural(inputs.unsupported, "other file")} left alone`);
  return parts.join(" · ");
}

/**
 * The left column: the files list. Empty, it's the drop zone; with files,
 * it is the queue too: the same list shows what will happen, what is
 * happening and what happened, filtered by the chips above it.
 */
export function FilesPanel({ inputs }: { inputs: InputsSummary }) {
  const filter = useSession((state) => state.filter);
  const scanning = useSession((state) => state.scan !== null);
  const locked = useSession((state) => isRunActive(state.run)) || scanning;

  if (inputs.roots.length === 0) {
    return (
      <section className={styles.panel} aria-labelledby="files-heading">
        <h2 id="files-heading" className={styles.heading}>
          Files
        </h2>
        <div className={styles.dropZone}>
          <p className={styles.dropTitle}>Drop images or folders here</p>
          <p className={styles.dropHint}>
            Mixed formats are fine: each file is recognised by its content. Folders keep their structure. Nothing is
            changed until you press Convert.
          </p>
          <div className={styles.actions}>
            <AddButtons disabled={locked} />
          </div>
          <ScanStatus />
        </div>
      </section>
    );
  }

  return (
    <section className={styles.panel} aria-labelledby="files-heading">
      <div className={styles.toolbar}>
        <div className={styles.title}>
          <h2 id="files-heading" className={styles.heading}>
            Files
          </h2>
          <span className={styles.summary}>{summaryLine(inputs)}</span>
        </div>
        <div className={styles.actions}>
          <AddButtons disabled={locked} />
          <ClearMenu disabled={locked} />
        </div>
      </div>
      <ScanStatus />
      {inputs.warnings.length > 0 && (
        <ul className={styles.warnings}>
          {inputs.warnings.map((warning) => (
            <li key={warning}>{warning}</li>
          ))}
        </ul>
      )}
      <FilterChips />
      {filter === "all" ? <TreeList /> : <FlatList filter={filter} />}
    </section>
  );
}

import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { GearIcon } from "../../components/icons";
import type { Preset, TargetTotals } from "@bindings";
import { FORMAT_DESCRIPTIONS, defaultsFor, effective, overrideNotes, settingsSummary } from "../../lib/codecs";
import { FORMAT_LABELS, OUTPUT_FORMATS, writableFormats } from "../../lib/formats";
import {
  EMPTY_NAMING,
  FACTORY_COLLISION,
  FACTORY_DESTINATION,
  FACTORY_METADATA,
  pathTail,
  splitName,
} from "../../lib/policies";
import { formatBytes, formatCount } from "../../lib/units";
import { isRunActive } from "../../state/session";
import { usePlatform, useSession } from "../../state/sessionContext";
import { CollisionRow, FileNamesRow, MetadataRow, SaveToRow } from "../policies/PolicyRows";
import styles from "./OutputsPanel.module.css";

function AddOutputMenu({ preset }: { preset: Preset }) {
  const tools = useSession((state) => state.tools);
  const addOutput = useSession((state) => state.addOutput);
  const writable = writableFormats(tools);
  const taken = new Set(preset.outputs.map((output) => output.codec.format));

  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button type="button" className={`button ${styles.add}`}>
          <span aria-hidden="true">+</span> Add output
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className={styles.menu} align="start" sideOffset={4}>
          {OUTPUT_FORMATS.map((format) => (
            <DropdownMenu.Item
              key={format}
              className={styles.menuItem}
              disabled={!writable.has(format)}
              onSelect={() => void addOutput(format)}
            >
              <span className={styles.menuName}>
                {FORMAT_LABELS[format]}
                {taken.has(format) && <span className={styles.menuTag}>added</span>}
              </span>
              <span className={styles.menuNote}>
                {writable.has(format) ? FORMAT_DESCRIPTIONS[format] : "No installed tool writes it. See Tools."}
              </span>
            </DropdownMenu.Item>
          ))}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

/**
 * How far one output is in the current run, and once it's finished what it
 * wrote. Counts every planned file of that output, so a second run of the
 * files left over continues from the first.
 */
function CardProgress({ label, totals, finished }: { label: string; totals: TargetTotals; finished: boolean }) {
  const done = totals.converted + totals.failed + totals.cancelled;
  const total = done + totals.ready;
  return (
    <div className={styles.progress}>
      <div className={styles.progressRow}>
        <div
          className={styles.track}
          role="progressbar"
          aria-label={`${label} progress`}
          aria-valuemin={0}
          aria-valuemax={total}
          aria-valuenow={done}
        >
          <div className={styles.fill} style={{ width: `${total === 0 ? 100 : (done / total) * 100}%` }} />
        </div>
        <span className={styles.count}>
          {formatCount(done)} of {formatCount(total)}
        </span>
      </div>
      {finished ? (
        <span className={styles.results}>{totalsLine(totals)}</span>
      ) : (
        totals.failed > 0 && <span className={styles.failed}>{formatCount(totals.failed)} failed</span>
      )}
    </div>
  );
}

function OutputCards({ preset, locked }: { preset: Preset; locked: boolean }) {
  const codecs = useSession((state) => state.codecs);
  const run = useSession((state) => state.run);
  const removeOutput = useSession((state) => state.removeOutput);
  const editOutput = useSession((state) => state.editOutput);
  if (preset.outputs.length === 0) {
    return <p className={styles.empty}>Pick at least one format to convert to.</p>;
  }
  return (
    <ul className={styles.cards}>
      {preset.outputs.map((output, index) => {
        const label = FORMAT_LABELS[output.codec.format];
        const codec = defaultsFor(codecs, output.codec.format);
        const summary = codec ? settingsSummary(effective(codec.inherited, output.codec)) : "";
        const notes = overrideNotes(output);
        const totals = run?.progress.targets.find((t) => t.target === index);
        return (
          <li key={`${output.codec.format}-${index}`} className={styles.card}>
            <button
              type="button"
              className={styles.cardText}
              aria-label={`${label} settings`}
              onClick={() => editOutput(index)}
            >
              <span className={styles.format}>{label}</span>
              <span className={styles.settings}>{summary}</span>
              {notes.length > 0 && <span className={styles.overrides}>{notes.join(" · ")}</span>}
            </button>
            <button
              type="button"
              className={styles.icon}
              aria-label={`Edit ${label} settings`}
              title="Settings"
              onClick={() => editOutput(index)}
            >
              <GearIcon />
            </button>
            <button
              type="button"
              className={styles.icon}
              aria-label={`Remove ${label} output`}
              title={locked ? "Locked while converting" : "Remove"}
              disabled={locked}
              onClick={() => void removeOutput(index)}
            >
              ×
            </button>
            {totals && <CardProgress label={label} totals={totals} finished={run?.state === "finished"} />}
          </li>
        );
      })}
    </ul>
  );
}

/** The four preset-wide policies, plus where the first file would go. */
function WhereAndHow({ preset, locked }: { preset: Preset; locked: boolean }) {
  const defaults = useSession((state) => state.defaults);
  const example = useSession((state) => state.overview?.plan?.examples[0]);
  const setPolicies = useSession((state) => state.setPolicies);
  const format = preset.outputs[0]?.codec.format ?? "webp";
  return (
    <section className={styles.where} aria-labelledby="where-heading">
      <h3 id="where-heading" className={styles.subheading}>
        Where and how
      </h3>
      <div className={styles.rows}>
        <SaveToRow
          value={preset.destination}
          fallback={defaults.destination ?? FACTORY_DESTINATION}
          disabled={locked}
          onChange={(destination) => void setPolicies({ destination })}
        />
        <FileNamesRow
          value={preset.naming}
          fallback={defaults.naming ?? EMPTY_NAMING}
          disabled={locked}
          example={example ? splitName(example.source) : { stem: "beach", ext: "jpg" }}
          format={format}
          onChange={(naming) => void setPolicies({ naming })}
        />
        <CollisionRow
          value={preset.collision}
          fallback={defaults.collision ?? FACTORY_COLLISION}
          disabled={locked}
          onChange={(collision) => void setPolicies({ collision })}
        />
        <MetadataRow
          value={preset.metadata}
          fallback={defaults.metadata ?? FACTORY_METADATA}
          disabled={locked}
          onChange={(metadata) => void setPolicies({ metadata })}
        />
      </div>
      {example && (
        <p className={styles.example} title={example.output}>
          Example: {pathTail(example.output, 3)}
        </p>
      )}
    </section>
  );
}

/** What a finished run wrote for one output. */
function totalsLine(totals: TargetTotals): string {
  const parts = [`${formatCount(totals.converted)} written`];
  if (totals.converted > 0) parts.push(`${formatBytes(totals.inputBytes)} → ${formatBytes(totals.outputBytes)}`);
  if (totals.skipped > 0) parts.push(`${formatCount(totals.skipped)} skipped`);
  if (totals.failed > 0) parts.push(`${formatCount(totals.failed)} failed`);
  if (totals.cancelled > 0) parts.push(`${formatCount(totals.cancelled)} cancelled`);
  return parts.join(" · ");
}

/** The finished run's report, with every file's result. */
function SaveReport() {
  const platform = usePlatform();
  const finished = useSession((state) => state.run?.state === "finished");
  const saveReport = useSession((state) => state.saveReport);
  if (!finished) return null;
  const save = async () => {
    const path = await platform.pickReportPath();
    if (path) await saveReport(path);
  };
  return (
    <button type="button" className="link" onClick={() => void save()}>
      Save report…
    </button>
  );
}

/** The right column: one card per output, then where and how files are written. */
export function OutputsPanel({ preset }: { preset: Preset }) {
  const locked = useSession((state) => isRunActive(state.run));

  return (
    <section className={styles.panel} aria-labelledby="outputs-heading">
      <h2 id="outputs-heading" className={styles.heading}>
        Outputs
      </h2>
      {locked && (
        <p className={styles.lock} role="note">
          Settings are locked while converting. Cancel, or wait for the run to finish, to change them.
        </p>
      )}
      <OutputCards preset={preset} locked={locked} />
      {!locked && (
        <div className={styles.belowCards}>
          <AddOutputMenu preset={preset} />
          <SaveReport />
        </div>
      )}
      <WhereAndHow preset={preset} locked={locked} />
    </section>
  );
}

import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import type { Destination, Preset } from "@bindings";
import { FORMAT_LABELS, OUTPUT_FORMATS, outputSummary, writableFormats } from "../../lib/formats";
import { isRunActive } from "../../state/session";
import { usePlatform, useSession } from "../../state/sessionContext";
import styles from "./OutputsPanel.module.css";

const BESIDE_SOURCE: Destination = { mode: "besideSource" };

function AddOutputMenu({ preset, disabled }: { preset: Preset; disabled: boolean }) {
  const tools = useSession((state) => state.tools);
  const addOutput = useSession((state) => state.addOutput);
  const writable = writableFormats(tools);
  // One output per format until per-output naming and destinations (G3)
  // can keep two of the same format apart.
  const taken = new Set(preset.outputs.map((output) => output.codec.format));
  const choices = OUTPUT_FORMATS.filter((format) => !taken.has(format));

  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button type="button" className="button" disabled={disabled || choices.length === 0}>
          Add output
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className={styles.menu} align="start" sideOffset={4}>
          {choices.map((format) => (
            <DropdownMenu.Item
              key={format}
              className={styles.menuItem}
              disabled={!writable.has(format)}
              onSelect={() => void addOutput(format)}
            >
              {FORMAT_LABELS[format]}
              {!writable.has(format) && <span className={styles.menuNote}>No installed tool writes it</span>}
            </DropdownMenu.Item>
          ))}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

function DestinationPicker({ preset, disabled }: { preset: Preset; disabled: boolean }) {
  const platform = usePlatform();
  const defaults = useSession((state) => state.defaults);
  const setDestination = useSession((state) => state.setDestination);
  // Unset in the working preset means the user's default, then the factory one.
  const current = preset.destination ?? defaults.destination ?? BESIDE_SOURCE;

  const chooseCustom = async () => {
    const root = await platform.pickDestination();
    if (root === null) return;
    const perFormatSubfolders = current.mode === "custom" ? current.perFormatSubfolders : false;
    await setDestination({ mode: "custom", root, perFormatSubfolders });
  };

  return (
    // The divider is on a wrapper: a fieldset draws its legend on its border.
    <div className={styles.destination}>
      <fieldset className={styles.fieldset} disabled={disabled}>
        <legend className={styles.subheading}>Destination</legend>
        <label className={styles.option}>
          <input
            type="radio"
            name="destination"
            checked={current.mode === "besideSource"}
            onChange={() => void setDestination(BESIDE_SOURCE)}
          />
          Beside each source
        </label>
        <label className={styles.option}>
          <input
            type="radio"
            name="destination"
            checked={current.mode === "subfolder"}
            onChange={() => void setDestination({ mode: "subfolder" })}
          />
          A subfolder per format, inside each source folder
        </label>
        <label className={styles.option}>
          <input
            type="radio"
            name="destination"
            checked={current.mode === "custom"}
            onChange={() => void chooseCustom()}
          />
          A folder of my choice
        </label>
        {current.mode === "custom" && (
          <div className={styles.custom}>
            <p className={styles.root} title={current.root}>
              {current.root}
            </p>
            <button type="button" className="button" onClick={() => void chooseCustom()}>
              Change…
            </button>
            <label className={styles.option}>
              <input
                type="checkbox"
                checked={current.perFormatSubfolders}
                onChange={(event) => void setDestination({ ...current, perFormatSubfolders: event.target.checked })}
              />
              Separate folder per format
            </label>
            <p className={styles.hint}>Subfolders of what you added are kept, so files from different folders don't mix.</p>
          </div>
        )}
      </fieldset>
    </div>
  );
}

/** The right column: one card per output, then where files will land. */
export function OutputsPanel({ preset }: { preset: Preset }) {
  const removeOutput = useSession((state) => state.removeOutput);
  const locked = useSession((state) => isRunActive(state.run));

  return (
    <section className={styles.panel} aria-labelledby="outputs-heading">
      <h2 id="outputs-heading" className={styles.heading}>
        Outputs
      </h2>
      {preset.outputs.length === 0 ? (
        <p className={styles.empty}>Pick at least one format to convert to.</p>
      ) : (
        <ul className={styles.cards}>
          {preset.outputs.map((output, index) => {
            const label = FORMAT_LABELS[output.codec.format];
            return (
              <li key={`${output.codec.format}-${index}`} className={styles.card}>
                <div className={styles.cardText}>
                  <span className={styles.format}>{label}</span>
                  <span className={styles.settings}>{outputSummary(output)}</span>
                </div>
                <button
                  type="button"
                  className={styles.remove}
                  aria-label={`Remove ${label} output`}
                  disabled={locked}
                  onClick={() => void removeOutput(index)}
                >
                  ×
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <AddOutputMenu preset={preset} disabled={locked} />
      <DestinationPicker preset={preset} disabled={locked} />
    </section>
  );
}

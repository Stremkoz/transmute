import * as Dialog from "@radix-ui/react-dialog";
import { GearIcon } from "../../components/icons";
import { useState } from "react";
import type { Theme, UserDefaults } from "@bindings";
import dialog from "../../components/Dialog.module.css";
import { InfoTip } from "../../components/InfoTip";
import { defaultsFor, effective, settingsSummary } from "../../lib/codecs";
import { FORMAT_LABELS } from "../../lib/formats";
import {
  EMPTY_NAMING,
  FACTORY_COLLISION,
  FACTORY_DESTINATION,
  FACTORY_METADATA,
  isIdentityNaming,
} from "../../lib/policies";
import { isRunActive } from "../../state/session";
import { useSession } from "../../state/sessionContext";
import { CollisionRow, FileNamesRow, MetadataRow, SaveToRow } from "../policies/PolicyRows";
import styles from "./SettingsDialog.module.css";

const MAX_WORKERS = 256;

const THEMES: { value: Theme | undefined; label: string }[] = [
  { value: undefined, label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/** Light, dark, or as Windows is. Changes even while converting. */
function Appearance() {
  const theme = useSession((state) => state.settings.theme);
  const setTheme = useSession((state) => state.setTheme);
  return (
    <section className={styles.section}>
      <h3 className={styles.heading}>Appearance</h3>
      <div className={styles.segmented} role="group" aria-label="Appearance">
        {THEMES.map((option) => (
          <button
            key={option.label}
            type="button"
            aria-pressed={theme === option.value}
            onClick={() => void setTheme(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
      {theme === undefined && <p className={styles.help}>Follows the Windows setting, and changes when it does.</p>}
    </section>
  );
}

function Workers({ disabled }: { disabled: boolean }) {
  const settings = useSession((state) => state.settings);
  const setSettings = useSession((state) => state.setSettings);
  const [draft, setDraft] = useState(String(settings.workers ?? ""));
  const automatic = settings.workers === undefined;
  const cores = navigator.hardwareConcurrency || 4;

  const commit = (text: string) => {
    setDraft(text);
    const value = Number(text);
    if (Number.isInteger(value) && value >= 1 && value <= MAX_WORKERS) void setSettings({ ...settings, workers: value });
  };

  return (
    <section className={styles.section}>
      <h3 className={styles.heading}>
        Conversions at once
        <InfoTip label="conversions at once">
          How many files are converted in parallel. One per processor core keeps the computer busy without
          slowing it to a crawl; fewer leaves room for other work.
        </InfoTip>
      </h3>
      <label className={styles.check}>
        <input
          type="checkbox"
          checked={automatic}
          disabled={disabled}
          onChange={(event) =>
            void setSettings(
              event.target.checked
                ? { ...settings, workers: undefined }
                : { ...settings, workers: Math.max(1, Math.floor(cores / 2)) },
            )
          }
        />
        Automatic: one per processor core ({cores})
      </label>
      {!automatic && (
        <label className={styles.inline}>
          <input
            type="number"
            className={styles.number}
            min={1}
            max={MAX_WORKERS}
            value={draft || String(settings.workers)}
            disabled={disabled}
            onChange={(event) => commit(event.target.value)}
          />
          at once
        </label>
      )}
    </section>
  );
}

function PolicyDefaults({ disabled }: { disabled: boolean }) {
  const defaults = useSession((state) => state.defaults);
  const setDefaults = useSession((state) => state.setDefaults);
  const change = (patch: Partial<UserDefaults>) => void setDefaults({ ...defaults, ...patch });
  return (
    <section className={styles.section}>
      <h3 className={styles.heading}>Where and how, by default</h3>
      <p className={styles.help}>Used by every setup and preset that doesn't choose for itself.</p>
      <div className={styles.rows}>
        <SaveToRow
          value={defaults.destination}
          fallback={FACTORY_DESTINATION}
          fallbackTag="factory"
          disabled={disabled}
          onChange={(destination) => change({ destination })}
        />
        <FileNamesRow
          value={defaults.naming}
          fallback={EMPTY_NAMING}
          fallbackTag="factory"
          disabled={disabled}
          example={{ stem: "beach", ext: "jpg" }}
          format="webp"
          onChange={(naming) => change({ naming: naming && !isIdentityNaming(naming) ? naming : undefined })}
        />
        <CollisionRow
          value={defaults.collision}
          fallback={FACTORY_COLLISION}
          fallbackTag="factory"
          disabled={disabled}
          onChange={(collision) => change({ collision })}
        />
        <MetadataRow
          value={defaults.metadata}
          fallback={FACTORY_METADATA}
          fallbackTag="factory"
          disabled={disabled}
          onChange={(metadata) => change({ metadata })}
        />
      </div>
    </section>
  );
}

function CodecDefaults({ disabled }: { disabled: boolean }) {
  const defaults = useSession((state) => state.defaults);
  const codecs = useSession((state) => state.codecs);
  const setDefaults = useSession((state) => state.setDefaults);
  const patches = defaults.codecs ?? [];
  return (
    <section className={styles.section}>
      <h3 className={styles.heading}>Format defaults</h3>
      {patches.length === 0 ? (
        <p className={styles.help}>
          Every format uses Transmute's own settings. To change one, open an output's settings and choose Make these my
          defaults.
        </p>
      ) : (
        <ul className={styles.codecs}>
          {patches.map((patch) => {
            const codec = defaultsFor(codecs, patch.format);
            return (
              <li key={patch.format} className={styles.codec}>
                <div>
                  <span className={styles.codecName}>{FORMAT_LABELS[patch.format]}</span>
                  {codec && (
                    <span className={styles.help}>
                      Yours: {settingsSummary(effective(codec.factory, patch))}
                      <br />
                      Factory: {settingsSummary(effective(codec.factory, { format: patch.format } as typeof patch))}
                    </span>
                  )}
                </div>
                <button
                  type="button"
                  className="button"
                  disabled={disabled}
                  onClick={() =>
                    void setDefaults({ ...defaults, codecs: patches.filter((c) => c.format !== patch.format) })
                  }
                >
                  Use factory settings
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

/** ⚙ Settings: the user's defaults and how many files convert at once. */
export function SettingsDialog() {
  const locked = useSession((state) => isRunActive(state.run));
  return (
    <Dialog.Root>
      <Dialog.Trigger asChild>
        <button type="button" className="button" aria-label="Settings" title="Settings">
          <GearIcon />
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className={dialog.overlay} />
        <Dialog.Content className={dialog.dialog}>
          <Dialog.Title className={dialog.title}>Settings</Dialog.Title>
          <Dialog.Description className={dialog.description}>
            {locked ? "Locked while converting. Cancel, or wait for the run to finish, to change them." : "Saved as you change them."}
          </Dialog.Description>
          <Appearance />
          <Workers disabled={locked} />
          <PolicyDefaults disabled={locked} />
          <CodecDefaults disabled={locked} />
          <div className={dialog.buttons}>
            <Dialog.Close asChild>
              <button type="button" className="button">
                Close
              </button>
            </Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

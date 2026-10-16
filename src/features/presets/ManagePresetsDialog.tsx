import * as Dialog from "@radix-ui/react-dialog";
import { useState, type FormEvent } from "react";
import type { StoredPreset } from "@bindings";
import { usePlatform, useSession } from "../../state/sessionContext";
import dialog from "../../components/Dialog.module.css";
import styles from "./Presets.module.css";
import { presetLine } from "./presetText";

function PresetRow({ stored }: { stored: StoredPreset }) {
  const platform = usePlatform();
  const defaults = useSession((state) => state.defaults);
  const loadedId = useSession((state) => state.overview?.loaded?.id);
  const renamePreset = useSession((state) => state.renamePreset);
  const deletePreset = useSession((state) => state.deletePreset);
  const exportPreset = useSession((state) => state.exportPreset);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const { id, preset } = stored;

  const rename = async (event: FormEvent) => {
    event.preventDefault();
    const name = renaming?.trim();
    if (name && name !== preset.name) await renamePreset(id, name);
    setRenaming(null);
  };
  const exportFile = async () => {
    const path = await platform.pickPresetExportPath(preset.name);
    if (path) await exportPreset(id, path);
  };

  return (
    <li className={styles.manageRow}>
      {renaming === null ? (
        <div className={styles.manageText}>
          <span className={styles.itemName}>
            {preset.name}
            {loadedId === id && <span className={styles.inUse}>in use</span>}
          </span>
          <span className={styles.itemNote}>{presetLine(preset, defaults)}</span>
        </div>
      ) : (
        <form className={styles.renameForm} onSubmit={(event) => void rename(event)}>
          <input
            className={styles.input}
            aria-label={`New name for ${preset.name}`}
            value={renaming}
            autoFocus
            onChange={(event) => setRenaming(event.target.value)}
            onKeyDown={(event) => event.key === "Escape" && (event.stopPropagation(), setRenaming(null))}
          />
          <button type="submit" className="button" disabled={!renaming.trim()}>
            Rename
          </button>
        </form>
      )}
      {renaming === null && !confirming && (
        <div className={styles.manageActions}>
          <button type="button" className="button" onClick={() => setRenaming(preset.name)}>
            Rename
          </button>
          <button type="button" className="button" onClick={() => void exportFile()}>
            Export…
          </button>
          <button type="button" className="button" onClick={() => setConfirming(true)}>
            Delete
          </button>
        </div>
      )}
      {confirming && (
        <div className={styles.manageActions} role="alert">
          <span className={styles.note}>
            Delete “{preset.name}”?{loadedId === id && " Your current setup stays, unsaved."}
          </span>
          <button type="button" className="button" onClick={() => setConfirming(false)}>
            Keep
          </button>
          <button type="button" className="button button-danger" onClick={() => void deletePreset(id)}>
            Delete
          </button>
        </div>
      )}
    </li>
  );
}

/** Rename, delete, import and export stored presets. */
export function ManagePresetsDialog({ open, onClose }: { open: boolean; onClose(): void }) {
  const platform = usePlatform();
  const presets = useSession((state) => state.presets);
  const importPreset = useSession((state) => state.importPreset);
  const importFile = async () => {
    const path = await platform.pickPresetFile();
    if (path) await importPreset(path);
  };
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className={dialog.overlay} />
        <Dialog.Content className={dialog.dialog}>
          <Dialog.Title className={dialog.title}>Presets</Dialog.Title>
          <Dialog.Description className={dialog.description}>
            Presets are saved setups: outputs plus where and how files are written. Share one by exporting it; a
            friend imports the file.
          </Dialog.Description>
          {presets.length === 0 ? (
            <p className={styles.menuEmpty}>No saved presets yet. Set up outputs, then use Save as a new preset.</p>
          ) : (
            <ul className={styles.manageList}>
              {presets.map((stored) => (
                <PresetRow key={stored.id} stored={stored} />
              ))}
            </ul>
          )}
          <div className={dialog.buttons}>
            <button type="button" className="button" onClick={() => void importFile()}>
              Import…
            </button>
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

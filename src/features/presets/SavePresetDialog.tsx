import * as Dialog from "@radix-ui/react-dialog";
import { useState, type FormEvent } from "react";
import type { Preset } from "@bindings";
import { FORMAT_LABELS } from "../../lib/formats";
import {
  EMPTY_NAMING,
  FACTORY_COLLISION,
  FACTORY_DESTINATION,
  FACTORY_METADATA,
  collisionLabel,
  destinationLabel,
  metadataLabel,
  namingLabel,
} from "../../lib/policies";
import { useSession } from "../../state/sessionContext";
import dialog from "../../components/Dialog.module.css";
import styles from "./Presets.module.css";
import { outputDetails } from "./presetText";

function formatsIn(preset: Preset): string {
  const filter = preset.inputFilter;
  if (!filter || filter.mode === "all") return "All images";
  const names = filter.formats.map((format) => FORMAT_LABELS[format]).join(", ");
  return filter.mode === "only" ? `Only ${names}` : `All but ${names}`;
}

function Form({ preset, onClose }: { preset: Preset; onClose(): void }) {
  const loaded = useSession((state) => state.overview?.loaded);
  const defaults = useSession((state) => state.defaults);
  const codecs = useSession((state) => state.codecs);
  const presets = useSession((state) => state.presets);
  const saveWorking = useSession((state) => state.saveWorking);
  // A copy of a loaded preset starts with its name; a new one with none.
  const [name, setName] = useState(loaded ? `${preset.name} copy` : "");
  const [description, setDescription] = useState(preset.description ?? "");
  const trimmed = name.trim();
  const taken = presets.some((stored) => stored.preset.name.toLowerCase() === trimmed.toLowerCase());

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!trimmed) return;
    if (await saveWorking(null, trimmed, description.trim() || null)) onClose();
  };

  return (
    <form className={styles.form} onSubmit={(event) => void submit(event)}>
      <label className={styles.field}>
        <span>Name</span>
        <input className={styles.input} value={name} autoFocus onChange={(event) => setName(event.target.value)} />
        {taken && <span className={styles.note}>Another preset has this name; both will be kept.</span>}
      </label>
      <label className={styles.field}>
        <span>
          Description <span className={styles.note}>(optional)</span>
        </span>
        <textarea
          className={styles.input}
          rows={2}
          value={description}
          onChange={(event) => setDescription(event.target.value)}
        />
      </label>
      <div className={styles.contains}>
        <p className={styles.containsTitle}>It will contain</p>
        <dl className={styles.facts}>
          <dt>Outputs</dt>
          <dd>
            {outputDetails(preset, codecs).map((line) => (
              <span key={line}>{line}</span>
            ))}
          </dd>
          <dt>Save to</dt>
          <dd>{destinationLabel(preset.destination ?? defaults.destination ?? FACTORY_DESTINATION)}</dd>
          <dt>File names</dt>
          <dd>{namingLabel(preset.naming ?? defaults.naming ?? EMPTY_NAMING)}</dd>
          <dt>If it exists</dt>
          <dd>{collisionLabel(preset.collision ?? defaults.collision ?? FACTORY_COLLISION)}</dd>
          <dt>Metadata</dt>
          <dd>{metadataLabel(preset.metadata ?? defaults.metadata ?? FACTORY_METADATA)}</dd>
          <dt>Formats in</dt>
          <dd>{formatsIn(preset)}</dd>
        </dl>
      </div>
      <div className={dialog.buttons}>
        <button type="button" className="button" onClick={onClose}>
          Cancel
        </button>
        <button type="submit" className="button button-primary" disabled={!trimmed}>
          Save preset
        </button>
      </div>
    </form>
  );
}

/** Saves the working setup as a new preset (mockup screen 9). */
export function SavePresetDialog({ open, onClose }: { open: boolean; onClose(): void }) {
  const preset = useSession((state) => state.overview?.preset);
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className={dialog.overlay} />
        <Dialog.Content className={dialog.dialog}>
          <Dialog.Title className={dialog.title}>Save as a new preset</Dialog.Title>
          <Dialog.Description className={dialog.description}>
            A preset remembers what to make and where it goes. It never includes the files.
          </Dialog.Description>
          {preset && <Form preset={preset} onClose={onClose} />}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

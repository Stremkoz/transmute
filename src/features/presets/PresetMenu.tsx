import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { ChevronDownIcon } from "../../components/icons";
import { useEffect, useState } from "react";
import { useSession } from "../../state/sessionContext";
import { ManagePresetsDialog } from "./ManagePresetsDialog";
import styles from "./Presets.module.css";
import { presetLine } from "./presetText";
import { SavePresetDialog } from "./SavePresetDialog";

/**
 * The header's Preset ▾: which preset the working setup came from (and
 * whether it's been edited), switching presets, and saving.
 */
export function PresetMenu() {
  const overview = useSession((state) => state.overview);
  const presets = useSession((state) => state.presets);
  const defaults = useSession((state) => state.defaults);
  const loadPreset = useSession((state) => state.loadPreset);
  const newPreset = useSession((state) => state.newPreset);
  const saveWorking = useSession((state) => state.saveWorking);
  const running = useSession((state) => state.run !== null && state.run.state !== "finished");
  const [saving, setSaving] = useState(false);
  const [managing, setManaging] = useState(false);

  const loaded = overview?.loaded ?? null;
  const preset = overview?.preset;
  const edited = loaded?.edited ?? false;
  const canSave = (preset?.outputs.length ?? 0) > 0;

  const saveChanges = () => {
    if (loaded && preset) void saveWorking(loaded.id, preset.name, preset.description ?? null);
  };

  // Ctrl+S saves changes (or asks for a name); Ctrl+Shift+S always asks.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "s") return;
      event.preventDefault();
      if (!canSave) return;
      if (event.shiftKey || !loaded) setSaving(true);
      else if (edited) saveChanges();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  if (!preset) return null;
  const name = loaded ? preset.name : "Untitled";

  return (
    <>
      <DropdownMenu.Root>
        <DropdownMenu.Trigger asChild>
          <button type="button" className={`button ${styles.trigger}`} aria-label={`Preset: ${name}${edited ? ", edited" : ""}`}>
            <span className={styles.triggerLabel}>Preset</span>
            <span className={styles.triggerName}>{name}</span>
            {edited && <span className={styles.edited}>edited</span>}
            <ChevronDownIcon />
          </button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content className={styles.menu} align="start" sideOffset={4}>
            <DropdownMenu.Label className={styles.menuLabel}>Presets</DropdownMenu.Label>
            {presets.length === 0 && <p className={styles.menuEmpty}>No saved presets yet.</p>}
            {presets.map((stored) => {
              const current = loaded?.id === stored.id;
              return (
                <DropdownMenu.Item
                  key={stored.id}
                  className={`${styles.item} ${current ? styles.current : ""}`}
                  disabled={running}
                  onSelect={() => void loadPreset(stored.id)}
                >
                  <span className={styles.itemName}>
                    {stored.preset.name}
                    {current && edited && <span className={styles.edited}>edited</span>}
                  </span>
                  <span className={styles.itemNote}>{presetLine(stored.preset, defaults)}</span>
                </DropdownMenu.Item>
              );
            })}
            <DropdownMenu.Item
              className={`${styles.item} ${loaded ? "" : styles.current}`}
              disabled={running}
              onSelect={() => void newPreset()}
            >
              <span className={styles.itemName}>Untitled (no preset)</span>
              <span className={styles.itemNote}>Start from your defaults, with no outputs picked</span>
            </DropdownMenu.Item>
            <DropdownMenu.Separator className={styles.separator} />
            <DropdownMenu.Item className={styles.action} disabled={!loaded || !edited} onSelect={saveChanges}>
              {loaded ? `Save changes to “${loaded.name}”` : "Save changes"}
              <kbd>Ctrl+S</kbd>
            </DropdownMenu.Item>
            <DropdownMenu.Item className={styles.action} disabled={!canSave} onSelect={() => setSaving(true)}>
              Save as a new preset…
              <kbd>Ctrl+Shift+S</kbd>
            </DropdownMenu.Item>
            <DropdownMenu.Item
              className={styles.action}
              disabled={!loaded || !edited || running}
              onSelect={() => loaded && void loadPreset(loaded.id)}
            >
              Undo my changes
            </DropdownMenu.Item>
            <DropdownMenu.Item className={styles.action} onSelect={() => setManaging(true)}>
              Manage presets…
              <span className={styles.itemNote}>rename, delete, import, export</span>
            </DropdownMenu.Item>
            <p className={styles.menuFoot}>
              Picking a preset swaps the outputs and Where-and-how settings. Your files stay in the list.
            </p>
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
      <SavePresetDialog open={saving} onClose={() => setSaving(false)} />
      <ManagePresetsDialog open={managing} onClose={() => setManaging(false)} />
    </>
  );
}

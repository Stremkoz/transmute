import type { NamingRule, OutputTarget, Preset } from "@bindings";
import { Drawer, DrawerSection } from "../../components/Drawer";
import { CODEC_FIELDS, NO_SETTINGS, defaultsFor, effective, isDefaultPatch, isSet, withField } from "../../lib/codecs";
import { FORMAT_LABELS } from "../../lib/formats";
import { EMPTY_NAMING, FACTORY_DESTINATION, FACTORY_METADATA, pathTail, splitName } from "../../lib/policies";
import { isRunActive } from "../../state/session";
import { useSession } from "../../state/sessionContext";
import { FileNamesRow, MetadataRow, SaveToRow } from "../policies/PolicyRows";
import { CodecField } from "./CodecField";
import styles from "./CodecDrawer.module.css";

const SAME = "Same as all outputs";

/** One output's settings, editable: its codec, plus what it does differently. */
function Editor({ preset, index, output }: { preset: Preset; index: number; output: OutputTarget }) {
  const codecs = useSession((state) => state.codecs);
  const defaults = useSession((state) => state.defaults);
  const example = useSession((state) => state.overview?.plan?.examples.find((e) => e.target === index));
  const updateOutput = useSession((state) => state.updateOutput);
  const locked = useSession((state) => isRunActive(state.run));
  const { format } = output.codec;
  const codec = defaultsFor(codecs, format);
  if (!codec) return <p className={styles.help}>Loading…</p>;

  const settings = effective(codec.inherited, output.codec);
  const fields = CODEC_FIELDS[format].filter((spec) => !spec.shown || spec.shown(settings));
  const update = (change: Partial<OutputTarget>) => void updateOutput(index, { ...output, ...change });
  const sample = example ? splitName(example.source) : { stem: "beach", ext: "jpg" };

  return (
    <>
      <DrawerSection label="Settings">
        {fields.length === 0 && <p className={styles.help}>{NO_SETTINGS[format]}</p>}
        {fields.map((spec) => (
          <CodecField
            key={spec.key}
            spec={spec}
            settings={settings}
            isSet={isSet(output.codec, spec.key)}
            disabled={locked}
            onSet={(value) => update({ codec: withField(output.codec, spec.key, value) })}
          />
        ))}
        {example && (
          <p className={styles.example} title={example.output}>
            Example: {pathTail(example.output, 3)}
          </p>
        )}
      </DrawerSection>
      <DrawerSection label="For this output only">
        <div className={styles.rows}>
          <SaveToRow
            value={output.destination}
            fallback={preset.destination ?? defaults.destination ?? FACTORY_DESTINATION}
            inheritLabel={SAME}
            disabled={locked}
            onChange={(destination) => update({ destination })}
          />
          <FileNamesRow
            value={output.naming}
            fallback={preset.naming ?? defaults.naming ?? (EMPTY_NAMING satisfies NamingRule)}
            inheritLabel={SAME}
            disabled={locked}
            example={sample}
            format={format}
            onChange={(naming) => update({ naming })}
          />
          <MetadataRow
            value={output.metadata}
            fallback={preset.metadata ?? defaults.metadata ?? FACTORY_METADATA}
            inheritLabel={SAME}
            disabled={locked}
            onChange={(metadata) => update({ metadata })}
          />
        </div>
        <p className={styles.help}>
          Two outputs of the same format need their own folder or file names, or they would write the same files.
        </p>
      </DrawerSection>
    </>
  );
}

/** The codec drawer: opened from an output card's ⚙. */
export function CodecDrawer() {
  const editing = useSession((state) => state.editing);
  const preset = useSession((state) => state.overview?.preset);
  const editOutput = useSession((state) => state.editOutput);
  const updateOutput = useSession((state) => state.updateOutput);
  const makeDefaults = useSession((state) => state.makeDefaults);
  const openViewer = useSession((state) => state.openViewer);
  const example = useSession((state) =>
    editing === null ? undefined : state.overview?.plan?.examples.find((e) => e.target === editing),
  );
  const locked = useSession((state) => isRunActive(state.run));
  const output = editing === null ? undefined : preset?.outputs[editing];
  const changed = output ? !isDefaultPatch(output.codec) : false;

  const footer = output && editing !== null && (
    <>
      <button
        type="button"
        className={`button ${styles.try}`}
        disabled={!example}
        title={
          example
            ? `Converts ${example.source} with these settings into a cache, and compares it with the original`
            : "Add images first: there's no file to try"
        }
        onClick={() => example && openViewer({ mode: "trial", entry: example.entry, target: editing })}
      >
        Try on one file…
      </button>
      <button
        type="button"
        className="button"
        disabled={locked || !changed}
        title={changed ? "Go back to your defaults for every setting" : "Every setting is already your default"}
        onClick={() => void updateOutput(editing, { ...output, codec: { format: output.codec.format } as OutputTarget["codec"] })}
      >
        Reset
      </button>
      <button
        type="button"
        className="button button-primary"
        disabled={locked || !changed}
        title={
          changed
            ? `New ${FORMAT_LABELS[output.codec.format]} outputs start with these settings`
            : "Nothing changed from your defaults"
        }
        onClick={() => void makeDefaults(editing)}
      >
        Make these my defaults
      </button>
    </>
  );

  return (
    <Drawer
      open={output !== undefined}
      onClose={() => editOutput(null)}
      title={output ? `${FORMAT_LABELS[output.codec.format]} settings` : "Output"}
      subtitle={
        preset && editing !== null && `Output ${editing + 1} of ${preset.outputs.length} · changes apply to this setup`
      }
      footer={footer}
    >
      {preset && output && editing !== null && <Editor preset={preset} index={editing} output={output} />}
    </Drawer>
  );
}


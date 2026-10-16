import * as Popover from "@radix-ui/react-popover";
import { useState, type ReactNode } from "react";
import type { CollisionPolicy, Destination, Format, MetadataPolicy, NamingRule } from "@bindings";
import { InfoTip } from "../../components/InfoTip";
import { ChevronDownIcon } from "../../components/icons";
import popover from "../../components/Popover.module.css";
import {
  COLLISION_OPTIONS,
  DATE_STYLES,
  METADATA_OPTIONS,
  NAME_TOKENS,
  collisionLabel,
  destinationLabel,
  isIdentityNaming,
  metadataLabel,
  namePartProblem,
  namingLabel,
  previewName,
  withNameParts,
} from "../../lib/policies";
import { usePlatform } from "../../state/sessionContext";
import styles from "./PolicyRows.module.css";

/**
 * The "Where and how" rows: one compact row per policy, each opening an
 * editor. Each editor gets the value set at its level (or `undefined`) and
 * the `fallback` that applies otherwise. With `inheritLabel` (an output's
 * "Same as all outputs"), following the fallback is a choice of its own,
 * reported as `undefined`.
 */

interface RowProps {
  label: string;
  info: ReactNode;
  value: string;
  /** Says where an unset value comes from. */
  tag?: string;
  disabled?: boolean;
  children: ReactNode;
}

export function PolicyRow({ label, info, value, tag, disabled, children }: RowProps) {
  return (
    <div className={styles.row}>
      <span className={styles.label}>
        {label}
        <InfoTip label={label}>{info}</InfoTip>
      </span>
      <Popover.Root>
        <Popover.Trigger asChild>
          <button type="button" className={styles.value} disabled={disabled} aria-label={`${label}: ${value}`}>
            <span className={styles.valueText}>{value}</span>
            {tag && <span className={styles.tag}>{tag}</span>}
            <span className={styles.chevron}>
              <ChevronDownIcon />
            </span>
          </button>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content className={popover.content} side="left" align="start" sideOffset={8}>
            <p className={styles.editorTitle}>{label}</p>
            {children}
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
    </div>
  );
}

interface EditorProps<T> {
  /** The value set at this level, if any. */
  value: T | undefined;
  /** What applies when nothing is set here. */
  fallback: T;
  /** Offer following the fallback, under this name. */
  inheritLabel?: string;
  /** What to call the fallback when it's shown (default: "default"). */
  fallbackTag?: string;
  disabled?: boolean;
  onChange(value: T | undefined): void;
}

function tagFor<T>({ value, inheritLabel, fallbackTag }: EditorProps<T>): string | undefined {
  if (value !== undefined) return undefined;
  return inheritLabel ? "same as all" : (fallbackTag ?? "default");
}

interface Choice<T> {
  value: T;
  label: string;
  help?: ReactNode;
}

/** Radio choices, plus the inherit choice when there is one (`undefined`). */
function Choices<T extends string>({
  name,
  choices,
  selected,
  inheritLabel,
  onChange,
}: {
  name: string;
  choices: Choice<T>[];
  selected: T | undefined;
  inheritLabel?: string;
  onChange(value: T | undefined): void;
}) {
  return (
    <div className={styles.choices} role="radiogroup" aria-label={name}>
      {inheritLabel && (
        <label className={styles.choice}>
          <input type="radio" name={name} checked={selected === undefined} onChange={() => onChange(undefined)} />
          <span>{inheritLabel}</span>
        </label>
      )}
      {choices.map((choice) => (
        <label key={choice.value} className={styles.choice}>
          <input type="radio" name={name} checked={selected === choice.value} onChange={() => onChange(choice.value)} />
          <span>
            {choice.label}
            {choice.help && <span className={styles.help}>{choice.help}</span>}
          </span>
        </label>
      ))}
    </div>
  );
}

/** What a radio group shows as chosen: inheriting is a choice only with `inheritLabel`. */
function selection<T>(props: EditorProps<T>): T | undefined {
  return props.inheritLabel ? props.value : (props.value ?? props.fallback);
}

export function SaveToRow(props: EditorProps<Destination>) {
  const { value, fallback, inheritLabel, disabled, onChange } = props;
  const platform = usePlatform();
  const shown = value ?? fallback;
  const chosen = selection(props);
  const chooseFolder = async () => {
    const root = await platform.pickDestination();
    if (root === null) return;
    onChange({ mode: "custom", root, perFormatSubfolders: shown.mode === "custom" && shown.perFormatSubfolders });
  };
  const choices: Choice<Destination["mode"]>[] = [
    { value: "besideSource", label: "Beside the originals", help: "photos/beach.jpg → photos/beach.webp" },
    { value: "subfolder", label: "A subfolder per format", help: "photos/beach.jpg → photos/webp/beach.webp" },
    {
      value: "custom",
      label: "A folder of my choice",
      help: chosen?.mode === "custom" ? <span className={styles.path}>{chosen.root}</span> : undefined,
    },
  ];
  const pick = (mode: Destination["mode"] | undefined) => {
    if (mode === undefined) onChange(undefined);
    else if (mode === "custom") void chooseFolder();
    else onChange({ mode });
  };
  return (
    <PolicyRow
      label="Save to"
      value={destinationLabel(shown)}
      tag={tagFor(props)}
      disabled={disabled}
      info="Where converted files go. Each file keeps its place relative to what you added, so files from different folders never mix."
    >
      <Choices name="Save to" choices={choices} selected={chosen?.mode} inheritLabel={inheritLabel} onChange={pick} />
      {chosen?.mode === "custom" && (
        <div className={styles.indent}>
          <button type="button" className="button" onClick={() => void chooseFolder()}>
            Change folder…
          </button>
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={chosen.perFormatSubfolders}
              onChange={(event) => onChange({ ...chosen, perFormatSubfolders: event.target.checked })}
            />
            A folder per format inside it
          </label>
        </div>
      )}
    </PolicyRow>
  );
}

export function CollisionRow(props: EditorProps<CollisionPolicy>) {
  return (
    <PolicyRow
      label="If it exists"
      value={collisionLabel(props.value ?? props.fallback)}
      tag={tagFor(props)}
      disabled={props.disabled}
      info="What happens when a file with the output's name is already there. Transmute checks before converting and never replaces an original."
    >
      <Choices
        name="If it exists"
        choices={COLLISION_OPTIONS}
        selected={selection(props)}
        inheritLabel={props.inheritLabel}
        onChange={props.onChange}
      />
    </PolicyRow>
  );
}

export function MetadataRow(props: EditorProps<MetadataPolicy>) {
  return (
    <PolicyRow
      label="Metadata"
      value={metadataLabel(props.value ?? props.fallback)}
      tag={tagFor(props)}
      disabled={props.disabled}
      info="Which details from the original are copied: camera, location, dates, copyright, colour profile. Where a format can't hold some of them, the plan says so before converting."
    >
      <Choices
        name="Metadata"
        choices={METADATA_OPTIONS}
        selected={selection(props)}
        inheritLabel={props.inheritLabel}
        onChange={props.onChange}
      />
    </PolicyRow>
  );
}

interface NamesProps extends EditorProps<NamingRule> {
  /** A file to show the result on. */
  example: { stem: string; ext: string };
  format: Format;
}

/** Prefix and suffix fields with tokens, and the name they make. */
function NamesEditor({ value, fallback, inheritLabel, onChange, example, format }: NamesProps) {
  const following = inheritLabel !== undefined && value === undefined;
  const [draft, setDraft] = useState(() => withNameParts(value ?? fallback));
  const [focused, setFocused] = useState<"prefix" | "suffix">("suffix");
  const problem = namePartProblem(draft.prefix) ?? namePartProblem(draft.suffix);

  // Half-typed tokens stay in the fields until they make sense.
  const commit = (next: Required<NamingRule>) => {
    setDraft(next);
    if (!(namePartProblem(next.prefix) ?? namePartProblem(next.suffix))) onChange(next);
  };
  const insert = (token: string) => commit({ ...draft, [focused]: `${draft[focused]}${token}` });

  return (
    <>
      {inheritLabel && (
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={following}
            onChange={(event) => onChange(event.target.checked ? undefined : draft)}
          />
          {inheritLabel}
        </label>
      )}
      {!following && (
        <>
          <div className={styles.nameFields}>
            <label className={styles.field}>
              <span>Before the name</span>
              <input
                className={styles.input}
                value={draft.prefix}
                spellCheck={false}
                onFocus={() => setFocused("prefix")}
                onChange={(event) => commit({ ...draft, prefix: event.target.value })}
              />
            </label>
            <span className={styles.nameMiddle}>‹name›</span>
            <label className={styles.field}>
              <span>After the name</span>
              <input
                className={styles.input}
                value={draft.suffix}
                spellCheck={false}
                onFocus={() => setFocused("suffix")}
                onChange={(event) => commit({ ...draft, suffix: event.target.value })}
              />
            </label>
          </div>
          <div className={styles.tokens} role="group" aria-label="Insert">
            {NAME_TOKENS.map((token) => (
              <button key={token.token} type="button" className={styles.token} onClick={() => insert(token.token)}>
                + {token.label}
              </button>
            ))}
          </div>
          {(draft.prefix + draft.suffix).includes("{date}") && (
            <div className={styles.dateFields}>
              <label className={styles.field}>
                <span>Date style</span>
                <select
                  className={styles.input}
                  value={draft.dateStyle}
                  onChange={(event) => commit({ ...draft, dateStyle: event.target.value as NamingRule["dateStyle"] })}
                >
                  {DATE_STYLES.map((style) => (
                    <option key={style.value} value={style.value}>
                      {style.label}
                    </option>
                  ))}
                </select>
              </label>
              <label className={styles.field}>
                <span>Which date</span>
                <select
                  className={styles.input}
                  value={draft.dateSource}
                  onChange={(event) => commit({ ...draft, dateSource: event.target.value as NamingRule["dateSource"] })}
                >
                  <option value="batchStart">When converting starts</option>
                  <option value="sourceModified">When the original was last changed</option>
                </select>
              </label>
            </div>
          )}
          {problem ? (
            <p className={styles.problem} role="alert">
              {problem}
            </p>
          ) : (
            <p className={styles.preview}>
              {example.stem}.{example.ext} → <b>{previewName(draft, example.stem, example.ext, format)}</b>
            </p>
          )}
          <p className={styles.help}>
            Names stay as they are unless you add something here. Number counts through the whole batch.
          </p>
        </>
      )}
    </>
  );
}

export function FileNamesRow(props: NamesProps) {
  const shown = props.value ?? props.fallback;
  return (
    <PolicyRow
      label="File names"
      value={namingLabel(shown)}
      tag={isIdentityNaming(shown) && !props.inheritLabel ? undefined : tagFor(props)}
      disabled={props.disabled}
      info="Converted files keep the original's name with the new extension. Add text before or after it, with tokens for the date, a number, or the format."
    >
      <NamesEditor {...props} />
    </PolicyRow>
  );
}

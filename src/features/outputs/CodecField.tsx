import { useEffect, useRef, useState } from "react";
import { modeOf, type FieldSpec, type Fields, type Mode } from "../../lib/codecs";
import styles from "./CodecDrawer.module.css";

interface FieldProps {
  spec: FieldSpec;
  /** The output's effective settings. */
  settings: Fields;
  /** Whether the output sets this field itself. */
  isSet: boolean;
  disabled: boolean;
  /** `undefined` goes back to the inherited value. */
  onSet(value: unknown): void;
}

/** How long a slider rests before its value is applied (each change re-plans). */
const SETTLE_MS = 250;

function NumberControl({ spec, settings, disabled, onSet }: FieldProps & { spec: Extract<FieldSpec, { kind: "number" }> }) {
  const current = Number(settings[spec.key]);
  const [draft, setDraft] = useState(current);
  // A new value from the backend (a reset, say) replaces the draft.
  const [applied, setApplied] = useState(current);
  if (applied !== current) {
    setApplied(current);
    setDraft(current);
  }
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  const change = (value: number, settle: boolean) => {
    if (!Number.isFinite(value)) return;
    const clamped = Math.min(spec.max, Math.max(spec.min, value));
    setDraft(clamped);
    clearTimeout(timer.current);
    if (settle) timer.current = setTimeout(() => onSet(clamped), SETTLE_MS);
    else onSet(clamped);
  };
  // Sliders reach the usual range; the number box takes the rest.
  const sliderMax = spec.marks ? Math.max(...spec.marks.map(([value]) => value)) : spec.max;

  return (
    <>
      <div className={styles.slider}>
        <input
          type="range"
          aria-label={spec.label}
          min={spec.min}
          max={Math.max(sliderMax, Math.min(draft, spec.max))}
          step={spec.step}
          value={draft}
          disabled={disabled}
          onChange={(event) => change(Number(event.target.value), true)}
        />
        <input
          type="number"
          className={styles.numberBox}
          aria-label={`${spec.label} value`}
          min={spec.min}
          max={spec.max}
          step={spec.step}
          value={draft}
          disabled={disabled}
          onChange={(event) => change(Number(event.target.value), true)}
        />
      </div>
      {spec.marks && (
        <div className={styles.marks}>
          {spec.marks.map(([value, label]) => (
            <button key={value} type="button" className={styles.mark} disabled={disabled} onClick={() => change(value, false)}>
              <b>{value}</b> {label}
            </button>
          ))}
        </div>
      )}
    </>
  );
}

function Control(props: FieldProps) {
  const { spec, settings, disabled, onSet } = props;
  const value = settings[spec.key];
  switch (spec.kind) {
    case "mode": {
      const current = modeOf(settings, spec);
      const modes: [Mode, string, unknown][] = [
        ["lossy", "Lossy", false],
        ["lossless", "Lossless", true],
        ...(spec.autoKey ? [["auto", "Auto", "auto"] as [Mode, string, unknown]] : []),
      ];
      return (
        <div className={styles.segmented} role="group" aria-label={spec.label}>
          {modes.map(([mode, label, set]) => (
            <button key={mode} type="button" aria-pressed={current === mode} disabled={disabled} onClick={() => onSet(set)}>
              {label}
            </button>
          ))}
        </div>
      );
    }
    case "bool":
      return (
        <label className={styles.switch}>
          <input type="checkbox" checked={value === true} disabled={disabled} onChange={(e) => onSet(e.target.checked)} />
          {value === true ? "On" : "Off"}
        </label>
      );
    case "number":
      return <NumberControl {...props} spec={spec} />;
    case "auto":
      return (
        <select
          className={styles.select}
          aria-label={spec.label}
          value={value === undefined || value === null ? "" : String(value)}
          disabled={disabled}
          onChange={(event) => onSet(event.target.value === "" ? undefined : Number(event.target.value))}
        >
          <option value="">Automatic</option>
          {Array.from({ length: spec.max - spec.min + 1 }, (_, i) => spec.min + i).map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
      );
    case "choice":
      return (
        <select
          className={styles.select}
          aria-label={spec.label}
          value={String(value)}
          disabled={disabled}
          onChange={(event) => onSet(event.target.value)}
        >
          {spec.options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      );
    case "colour": {
      const colour = typeof value === "string" ? value : "#ffffff";
      return (
        <div className={styles.colour}>
          <input type="color" aria-label={spec.label} value={colour} disabled={disabled} onChange={(e) => onSet(e.target.value)} />
          <span className={styles.hex}>{colour}</span>
          {["#ffffff", "#000000"].map((swatch) => (
            <button key={swatch} type="button" className={styles.mark} disabled={disabled} onClick={() => onSet(swatch)}>
              {swatch === "#ffffff" ? "White" : "Black"}
            </button>
          ))}
        </div>
      );
    }
  }
}

/** One codec setting: its control, where its value comes from, and what it does. */
export function CodecField(props: FieldProps) {
  const { spec, settings, isSet, disabled, onSet } = props;
  const automatic = spec.kind === "auto" && (settings[spec.key] === undefined || settings[spec.key] === null);
  return (
    <div className={styles.field}>
      <div className={styles.fieldHead}>
        <span className={styles.fieldLabel}>{spec.label}</span>
        {isSet ? (
          <button type="button" className={`link ${styles.reset}`} disabled={disabled} onClick={() => onSet(undefined)}>
            use default
          </button>
        ) : (
          <span className={styles.tag}>default</span>
        )}
      </div>
      <Control {...props} />
      <p className={styles.help}>
        {spec.help}
        {spec.kind === "auto" && automatic && ` ${spec.autoHelp}`}
      </p>
    </div>
  );
}

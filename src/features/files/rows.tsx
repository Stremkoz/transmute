import { useEffect, useRef } from "react";
import type { FileOutput } from "@bindings";
import { FORMAT_LABELS } from "../../lib/formats";
import { formatBytes } from "../../lib/units";
import styles from "./FilesList.module.css";

/** Short labels for output chips, where the full name is long. */
const SHORT: Partial<Record<FileOutput["format"], string>> = { jxl: "JXL" };

function chipLabel(output: FileOutput): { text: string; tone: string; title: string } {
  const name = SHORT[output.format] ?? FORMAT_LABELS[output.format];
  const { state } = output;
  switch (state.state) {
    case "ready":
      return { text: name, tone: "", title: `${FORMAT_LABELS[output.format]}: waiting to be converted` };
    case "running":
      return { text: `${name} converting`, tone: styles.run ?? "", title: "Converting now" };
    case "converted": {
      const saved = state.inputBytes > 0 ? Math.round((1 - state.outputBytes / state.inputBytes) * 100) : 0;
      return {
        text: `${name} ${formatBytes(state.outputBytes)}`,
        tone: styles.ok ?? "",
        title: `Written in ${state.seconds.toFixed(1)} s; ${saved >= 0 ? `${saved}% smaller` : `${-saved}% larger`}`,
      };
    }
    case "skipped":
      return {
        text: state.reason.code === "output-exists" ? `${name} exists` : `${name} skipped`,
        tone: styles.skip ?? "",
        title: state.reason.text,
      };
    case "blocked":
      return { text: `${name} can't`, tone: styles.fail ?? "", title: state.reason.text };
    case "failed":
      return { text: `! ${name} failed`, tone: styles.fail ?? "", title: state.message };
    case "cancelled":
      return { text: `${name} cancelled`, tone: styles.skip ?? "", title: "Cancelled before it was written" };
  }
}

export function OutputChips({ outputs }: { outputs: FileOutput[] }) {
  return (
    <>
      {outputs.map((output) => {
        const { text, tone, title } = chipLabel(output);
        return (
          <span key={output.job} className={`${styles.chip} ${tone}`} title={title}>
            {text}
          </span>
        );
      })}
    </>
  );
}

/** A checkbox that can show "some included". */
export function IncludeBox({
  label,
  checked,
  mixed = false,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  mixed?: boolean;
  disabled: boolean;
  onChange(include: boolean): void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = mixed;
  }, [mixed]);
  return (
    <input
      ref={ref}
      type="checkbox"
      className={styles.box}
      aria-label={label}
      checked={checked}
      disabled={disabled}
      onChange={(event) => onChange(event.target.checked)}
      onClick={(event) => event.stopPropagation()}
    />
  );
}

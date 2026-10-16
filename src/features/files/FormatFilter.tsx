import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import type { Format, InputFilter, InputsSummary } from "@bindings";
import { ChevronDownIcon } from "../../components/icons";
import { FORMAT_LABELS, OUTPUT_FORMATS } from "../../lib/formats";
import { formatCount } from "../../lib/units";
import { useSession } from "../../state/sessionContext";
import styles from "./FilesPanel.module.css";

/** The formats an input filter leaves out. */
function excludedBy(filter: InputFilter): Set<Format> {
  switch (filter.mode) {
    case "all":
      return new Set();
    case "exclude":
      return new Set(filter.formats);
    case "only":
      return new Set(OUTPUT_FORMATS.filter((format) => !filter.formats.includes(format)));
  }
}

function label(excluded: Set<Format>, present: Format[]): string {
  const left = present.filter((format) => excluded.has(format));
  if (left.length === 0) return "All formats";
  const kept = present.filter((format) => !excluded.has(format));
  if (kept.length === 1 && kept[0]) return `Only ${FORMAT_LABELS[kept[0]]}`;
  return `All but ${left.map((format) => FORMAT_LABELS[format]).join(", ")}`;
}

/**
 * Which source formats are converted (the preset's input filter). Unticking
 * a format leaves it out; formats added later are still included.
 */
export function FormatFilter({ inputs, disabled }: { inputs: InputsSummary; disabled: boolean }) {
  const filter = useSession((state) => state.overview?.preset.inputFilter ?? { mode: "all" as const });
  const setPolicies = useSession((state) => state.setPolicies);
  const excluded = excludedBy(filter);
  const present = OUTPUT_FORMATS.filter((format) => inputs.byFormat.some((count) => count.format === format));

  const toggle = (format: Format, include: boolean) => {
    const next = new Set(excluded);
    if (include) next.delete(format);
    else next.add(format);
    const formats = OUTPUT_FORMATS.filter((f) => next.has(f));
    void setPolicies({ inputFilter: formats.length === 0 ? { mode: "all" } : { mode: "exclude", formats } });
  };

  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger asChild>
        <button type="button" className="button" disabled={disabled || present.length === 0}>
          {label(excluded, present)}
          <ChevronDownIcon />
        </button>
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content className={styles.menu} align="end" sideOffset={4}>
          {present.map((format) => {
            const count = inputs.byFormat.find((c) => c.format === format)?.count ?? 0;
            return (
              <DropdownMenu.CheckboxItem
                key={format}
                className={styles.checkItem}
                checked={!excluded.has(format)}
                onCheckedChange={(checked) => toggle(format, checked)}
                onSelect={(event) => event.preventDefault()}
              >
                <span className={styles.checkBox} aria-hidden="true">
                  {!excluded.has(format) && "✓"}
                </span>
                {FORMAT_LABELS[format]}
                <span className={styles.faint}>{formatCount(count)}</span>
              </DropdownMenu.CheckboxItem>
            );
          })}
          <p className={styles.menuFoot}>Formats left out stay in the list, but nothing is made from them.</p>
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

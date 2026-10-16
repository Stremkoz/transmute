import * as Popover from "@radix-ui/react-popover";
import type { ReactNode } from "react";
import styles from "./Popover.module.css";

/** An ⓘ that explains the thing beside it (gui-plan: every number has one). */
export function InfoTip({ label, children }: { label: string; children: ReactNode }) {
  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button type="button" className={styles.info} aria-label={`About ${label}`}>
          i
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content className={`${styles.content} ${styles.tip}`} side="left" align="start" sideOffset={6}>
          {children}
          <Popover.Arrow className={styles.arrow} />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

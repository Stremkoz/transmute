import * as Dialog from "@radix-ui/react-dialog";
import type { ReactNode } from "react";
import styles from "./Drawer.module.css";

interface DrawerProps {
  open: boolean;
  onClose(): void;
  title: string;
  /** A line under the title (a path, "Output 1 of 2", …). */
  subtitle?: ReactNode;
  /** Pinned below the scrolling body. */
  footer?: ReactNode;
  children: ReactNode;
}

/** The panel that slides over the outputs column (gui-plan: one drawer, two uses). */
export function Drawer({ open, onClose, title, subtitle, footer, children }: DrawerProps) {
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className={styles.overlay} />
        <Dialog.Content className={styles.drawer} aria-describedby={undefined}>
          <header className={styles.header}>
            <div className={styles.headerText}>
              <Dialog.Title className={styles.title}>{title}</Dialog.Title>
              {subtitle && <p className={styles.subtitle}>{subtitle}</p>}
            </div>
            <Dialog.Close asChild>
              <button type="button" className={`button ${styles.close}`} aria-label="Close">
                ×
              </button>
            </Dialog.Close>
          </header>
          <div className={styles.body}>{children}</div>
          {footer && <footer className={styles.footer}>{footer}</footer>}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** One titled block of a drawer. */
export function DrawerSection({ label, children }: { label?: string; children: ReactNode }) {
  return (
    <section className={styles.section}>
      {label && <h3 className={styles.label}>{label}</h3>}
      {children}
    </section>
  );
}

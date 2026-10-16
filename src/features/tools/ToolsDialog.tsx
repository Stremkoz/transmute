import * as Dialog from "@radix-ui/react-dialog";
import type { ToolInfo } from "@bindings";
import { TOOL_LABELS } from "../../lib/formats";
import styles from "./ToolsDialog.module.css";

function statusText(info: ToolInfo): { text: string; tone: "ok" | "warn" | "error" } {
  const { status } = info;
  switch (status.status) {
    case "available":
      return status.outdated
        ? { text: `${status.version} (outdated)`, tone: "warn" }
        : { text: status.version, tone: "ok" };
    case "missing":
      return { text: "Not found", tone: "error" };
    case "unusable":
      return { text: status.reason, tone: "error" };
  }
}

function toolPath(info: ToolInfo): string | null {
  return info.status.status === "missing" ? null : info.status.path;
}

/** The installed encoders and decoders, as the backend found them. */
export function ToolsDialog({ tools }: { tools: ToolInfo[] }) {
  const missing = tools.filter((info) => info.status.status !== "available").length;
  return (
    <Dialog.Root>
      <Dialog.Trigger asChild>
        <button type="button" className="button">
          Tools{missing > 0 && <span className={styles.badge}>{missing} missing</span>}
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className={styles.overlay} />
        <Dialog.Content className={styles.content}>
          <Dialog.Title className={styles.title}>Tools</Dialog.Title>
          <Dialog.Description className={styles.description}>
            Transmute converts with these programs. Formats a missing tool would handle are planned through
            another tool, or reported as unavailable.
          </Dialog.Description>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">Tool</th>
                <th scope="col">Status</th>
                <th scope="col">Location</th>
              </tr>
            </thead>
            <tbody>
              {tools.map((info) => {
                const status = statusText(info);
                return (
                  <tr key={info.tool}>
                    <td>{TOOL_LABELS[info.tool]}</td>
                    <td className={styles[status.tone]}>{status.text}</td>
                    <td className={styles.path}>{toolPath(info) ?? "—"}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <Dialog.Close asChild>
            <button type="button" className={`button ${styles.close}`}>
              Close
            </button>
          </Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

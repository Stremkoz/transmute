import * as Dialog from "@radix-ui/react-dialog";
import { useState } from "react";
import type { ToolInfo } from "@bindings";
import { FORMAT_LABELS, OUTPUT_FORMATS, TOOL_LABELS, writableFormats } from "../../lib/formats";
import { SOURCE_LABELS, TOOL_PURPOSE, TOOL_SOURCES, onlyWriter, toolAbilities, toolProblems } from "../../lib/tools";
import { isRunActive } from "../../state/session";
import { usePlatform, useSession } from "../../state/sessionContext";
import styles from "./ToolsDialog.module.css";

function statusText(info: ToolInfo): { text: string; tone: "ok" | "warn" | "error" | "muted" } {
  const { status } = info;
  switch (status.status) {
    case "available":
      return status.outdated
        ? { text: `${status.version}, outdated`, tone: "warn" }
        : { text: status.version, tone: "ok" };
    case "missing":
      return { text: "Not installed", tone: "muted" };
    case "unusable":
      return { text: "Can't be used", tone: "error" };
  }
}

/** Copies a command, saying so for a moment. */
function CopyCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // The command stays selectable.
    }
  };
  return (
    <span className={styles.command}>
      <code>{command}</code>
      <button type="button" className="link" onClick={() => void copy()}>
        {copied ? "Copied" : "Copy"}
      </button>
    </span>
  );
}

function ToolRow({ info, tools }: { info: ToolInfo; tools: ToolInfo[] }) {
  const platform = usePlatform();
  const settings = useSession((state) => state.settings);
  const setSettings = useSession((state) => state.setSettings);
  const locked = useSession((state) => isRunActive(state.run));
  const status = statusText(info);
  const source = TOOL_SOURCES[info.tool];
  const chosen = settings.toolPaths[info.tool];
  const only = info.status.status === "available" ? onlyWriter(info, tools) : [];

  const choose = async () => {
    const path = await platform.pickToolPath(info.tool);
    if (path) await setSettings({ ...settings, toolPaths: { ...settings.toolPaths, [info.tool]: path } });
  };
  const forget = async () => {
    const toolPaths = { ...settings.toolPaths };
    delete toolPaths[info.tool];
    await setSettings({ ...settings, toolPaths });
  };

  return (
    <li className={styles.tool}>
      <div className={styles.toolHead}>
        <span className={styles.toolName}>{TOOL_LABELS[info.tool]}</span>
        <span className={styles[status.tone]}>{status.text}</span>
      </div>
      {info.status.status === "available" ? (
        <>
          <p className={styles.line}>{toolAbilities(info) || TOOL_PURPOSE[info.tool]}</p>
          {only.length > 0 && (
            <p className={styles.line}>
              The only installed tool that writes <b>{only.map((f) => FORMAT_LABELS[f]).join(", ")}</b>.
            </p>
          )}
          <p className={styles.line}>
            {SOURCE_LABELS[info.status.source] ?? info.status.source}:{" "}
            <span className={styles.path} title={info.status.path}>
              {info.status.path}
            </span>
          </p>
          {info.status.outdated && (
            <p className={styles.line}>An older version than Transmute was tested with; results may differ.</p>
          )}
        </>
      ) : (
        <>
          <p className={styles.line}>{TOOL_PURPOSE[info.tool]}</p>
          {info.status.status === "unusable" && (
            <p className={styles.problem}>
              {info.status.path}: {info.status.reason}
            </p>
          )}
          <p className={styles.line}>
            {source.bundled ? "Comes with Transmute; reinstalling restores it. Or get it from " : "Get it with "}
            {source.command && !source.bundled ? <CopyCommand command={source.command} /> : <span className={styles.url}>{source.url}</span>}
            {source.command && !source.bundled && (
              <>
                {" "}
                or from <span className={styles.url}>{source.url}</span>
              </>
            )}
            , then choose Look again.
          </p>
        </>
      )}
      {info.notes.length > 0 && (
        <ul className={styles.notes}>
          {info.notes.map((note) => (
            <li key={note}>{note}</li>
          ))}
        </ul>
      )}
      <div className={styles.actions}>
        <button type="button" className="button" disabled={locked} onClick={() => void choose()}>
          Use another copy…
        </button>
        {chosen !== undefined && (
          <button type="button" className="button" disabled={locked} onClick={() => void forget()}>
            Use the one Transmute finds
          </button>
        )}
      </div>
    </li>
  );
}

/** Formats nothing installed can write, for the summary. */
function unwritable(tools: ToolInfo[]) {
  const writable = writableFormats(tools);
  return OUTPUT_FORMATS.filter((format) => !writable.has(format));
}

/** The encoders and decoders Transmute uses: what each does, and how to fix or replace one. */
export function ToolsDialog({ tools }: { tools: ToolInfo[] }) {
  const refreshTools = useSession((state) => state.refreshTools);
  const locked = useSession((state) => isRunActive(state.run));
  const [looking, setLooking] = useState(false);
  const problems = toolProblems(tools);
  const missing = unwritable(tools);

  const lookAgain = async () => {
    setLooking(true);
    await refreshTools();
    setLooking(false);
  };

  return (
    <Dialog.Root>
      <Dialog.Trigger asChild>
        <button type="button" className="button">
          Tools{problems > 0 && <span className={styles.badge}>{problems} to fix</span>}
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className={styles.overlay} />
        <Dialog.Content className={styles.content}>
          <Dialog.Title className={styles.title}>Tools</Dialog.Title>
          <Dialog.Description className={styles.description}>
            Transmute converts with these programs. When several can do a job, it picks the best one for each file
            and says why in the file's details.
            {missing.length > 0 &&
              ` Nothing installed writes ${missing.map((f) => FORMAT_LABELS[f]).join(", ")}, so those outputs are greyed out.`}
          </Dialog.Description>
          <ul className={styles.tools}>
            {tools.map((info) => (
              <ToolRow key={info.tool} info={info} tools={tools} />
            ))}
          </ul>
          <div className={styles.footer}>
            <button type="button" className="button" disabled={locked || looking} onClick={() => void lookAgain()}>
              {looking ? "Looking…" : "Look again"}
            </button>
            <Dialog.Close asChild>
              <button type="button" className="button">
                Close
              </button>
            </Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

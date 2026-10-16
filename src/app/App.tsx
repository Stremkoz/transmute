import { useEffect, useState } from "react";
import { FileDrawer } from "../features/files/FileDrawer";
import { FilesPanel } from "../features/files/FilesPanel";
import { CompareViewer } from "../features/compare/CompareViewer";
import { CodecDrawer } from "../features/outputs/CodecDrawer";
import { PresetMenu } from "../features/presets/PresetMenu";
import { SettingsDialog } from "../features/settings/SettingsDialog";
import { ToolsDialog } from "../features/tools/ToolsDialog";
import { BottomBar } from "../features/workspace/BottomBar";
import { OutputsPanel } from "../features/workspace/OutputsPanel";
import { usePlatform, useSession } from "../state/sessionContext";
import { useTheme } from "./useTheme";
import styles from "./App.module.css";

// A stable empty list: a selector returning a fresh [] would re-render forever.
const NO_NOTICES: string[] = [];

function Notices() {
  const notices = useSession((state) => state.overview?.notices ?? NO_NOTICES);
  const dismiss = useSession((state) => state.dismissNotices);
  if (notices.length === 0) return null;
  return (
    <div className={styles.notices} role="status">
      <ul>
        {notices.map((text) => (
          <li key={text}>{text}</li>
        ))}
      </ul>
      <button type="button" className="button" onClick={() => void dismiss()}>
        Dismiss
      </button>
    </div>
  );
}

function CommandError() {
  const error = useSession((state) => state.commandError);
  const dismiss = useSession((state) => state.dismissError);
  if (!error) return null;
  return (
    <div className={styles.error} role="alert">
      <p>{error}</p>
      <button type="button" className="button" onClick={dismiss}>
        Dismiss
      </button>
    </div>
  );
}

/** Files dropped anywhere on the window are added; returns true while hovering. */
function useFileDrop(): boolean {
  const platform = usePlatform();
  const addInputs = useSession((state) => state.addInputs);
  const [hovering, setHovering] = useState(false);

  useEffect(() => {
    let stop: (() => void) | undefined;
    let active = true;
    void platform
      .onFileDrop({ onHover: setHovering, onDrop: (paths) => void addInputs(paths) })
      .then((unsubscribe) => {
        if (active) stop = unsubscribe;
        else unsubscribe();
      });
    return () => {
      active = false;
      stop?.();
    };
  }, [platform, addInputs]);

  return hovering;
}

/** The single conversion workspace (gui-plan: two columns plus a drawer). */
export function App() {
  useTheme();
  const status = useSession((state) => state.status);
  const overview = useSession((state) => state.overview);
  const tools = useSession((state) => state.tools);
  const error = useSession((state) => state.error);
  const hovering = useFileDrop();

  return (
    <div className={styles.app}>
      <header className={styles.header}>
        <h1 className={styles.title}>Transmute</h1>
        {status === "ready" && <PresetMenu />}
        <div className={styles.headerEnd}>
          <ToolsDialog tools={tools} />
          {status === "ready" && <SettingsDialog />}
        </div>
      </header>
      <Notices />
      <CommandError />
      {status === "loading" && <p className={styles.message}>Starting…</p>}
      {status === "failed" && (
        <p className={styles.message} role="alert">
          Transmute couldn't load its session: {error}
        </p>
      )}
      {status === "ready" && overview && (
        <>
          <main className={styles.workspace}>
            <FilesPanel inputs={overview.inputs} />
            <OutputsPanel preset={overview.preset} />
          </main>
          <BottomBar overview={overview} />
          <FileDrawer />
          <CodecDrawer />
          <CompareViewer />
        </>
      )}
      {hovering && (
        <div className={styles.dropOverlay} aria-hidden="true">
          <p>Drop to add</p>
        </div>
      )}
    </div>
  );
}

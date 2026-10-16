import { ToolsDialog } from "../features/tools/ToolsDialog";
import { ConvertBar } from "../features/workspace/ConvertBar";
import { InputsPanel } from "../features/workspace/InputsPanel";
import { OutputsPanel } from "../features/workspace/OutputsPanel";
import { useSession } from "../state/sessionContext";
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

/** The single conversion workspace (gui-plan: two columns plus a drawer). */
export function App() {
  const status = useSession((state) => state.status);
  const overview = useSession((state) => state.overview);
  const tools = useSession((state) => state.tools);
  const error = useSession((state) => state.error);

  return (
    <div className={styles.app}>
      <header className={styles.header}>
        <h1 className={styles.title}>Transmute</h1>
        <ToolsDialog tools={tools} />
      </header>
      <Notices />
      {status === "loading" && <p className={styles.message}>Starting…</p>}
      {status === "failed" && (
        <p className={styles.message} role="alert">
          Transmute couldn't load its session: {error}
        </p>
      )}
      {status === "ready" && overview && (
        <>
          <main className={styles.workspace}>
            <InputsPanel inputs={overview.inputs} />
            <OutputsPanel preset={overview.preset} />
          </main>
          <ConvertBar overview={overview} />
        </>
      )}
    </div>
  );
}

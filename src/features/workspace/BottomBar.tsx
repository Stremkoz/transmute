import type { Overview, PlanSummary } from "@bindings";
import { formatBytes, formatCount, formatDuration, plural } from "../../lib/units";
import { isRunActive, leftToConvert, type RunView } from "../../state/session";
import { useSession } from "../../state/sessionContext";
import styles from "./BottomBar.module.css";

/** Why there's nothing to plan yet, if there isn't. */
function blocker({ inputs, preset }: Overview): string | null {
  if (inputs.roots.length === 0) return "Add images, then pick at least one output.";
  if (inputs.convertible === 0) return "None of these files are images Transmute converts.";
  if (inputs.convertible <= inputs.excluded) return "Every image is excluded.";
  if (preset.outputs.length === 0) return "Pick an output format.";
  return null;
}

/** What the converted files saved, or by how much they grew (not shown as a saving). */
function savedText(progress: RunView["progress"]): { text: string; larger: boolean } | null {
  if (progress.converted === 0 || progress.inputBytes === 0) return null;
  const saved = progress.inputBytes - progress.outputBytes;
  const percent = Math.round((saved / progress.inputBytes) * 100);
  return saved >= 0
    ? { text: `saved ${formatBytes(saved)} (${percent}%)`, larger: false }
    : { text: `${formatBytes(-saved)} larger (${-percent}%)`, larger: true };
}

function PlanLine({ plan }: { plan: PlanSummary }) {
  const setFilter = useSession((state) => state.setFilter);
  const problems = plan.skipped + plan.blocked;
  return (
    <div className={styles.line}>
      <span className={styles.state}>Ready</span>
      <span className={styles.muted}>
        {plural(plan.sources, "image")} × {plural(plan.outputsPerSource, "output")} →{" "}
        <strong className={styles.strong}>{plural(plan.ready, "file")}</strong> to write
      </span>
      {plan.skipped > 0 && <span className={styles.warn}>· {formatCount(plan.skipped)} skipped</span>}
      {plan.blocked > 0 && <span className={styles.bad}>· {formatCount(plan.blocked)} can't be converted</span>}
      {problems > 0 && (
        <button type="button" className="link" onClick={() => setFilter("problems")}>
          Why?
        </button>
      )}
    </div>
  );
}

function Prepare({ overview }: { overview: Overview }) {
  const planning = useSession((state) => state.planning);
  const planError = useSession((state) => state.planError);
  const convert = useSession((state) => state.convert);
  const reason = blocker(overview);
  const ready = reason === null && !planning && (overview.plan?.ready ?? 0) > 0;
  let status;
  if (reason) status = <p className={styles.muted}>{reason}</p>;
  else if (planning) status = <p className={styles.muted}>Checking what will be converted…</p>;
  else if (planError) status = <p className={styles.bad}>{planError}</p>;
  else if (overview.plan) status = <PlanLine plan={overview.plan} />;

  return (
    <>
      <div className={styles.status} aria-live="polite">
        {status}
      </div>
      <button type="button" className={`button button-primary ${styles.primary}`} disabled={!ready} onClick={() => void convert()}>
        Convert
      </button>
    </>
  );
}

const RUN_TITLES: Record<RunView["state"], string> = {
  running: "Converting",
  paused: "Paused",
  cancelling: "Cancelling…",
  finished: "Finished",
};

function Running({ run }: { run: RunView }) {
  const pause = useSession((state) => state.pause);
  const resume = useSession((state) => state.resume);
  const cancel = useSession((state) => state.cancel);
  const setFilter = useSession((state) => state.setFilter);
  const { progress } = run;
  const done = progress.converted + progress.failed + progress.cancelled;
  const fraction = progress.total > 0 ? done / progress.total : 0;
  const saved = savedText(progress);

  return (
    <>
      <span className={styles.state}>{RUN_TITLES[run.state]}</span>
      <div
        className={styles.track}
        role="progressbar"
        aria-label="Conversion progress"
        aria-valuemin={0}
        aria-valuemax={progress.total}
        aria-valuenow={done}
      >
        <div className={styles.fill} style={{ width: `${(fraction * 100).toFixed(1)}%` }} />
      </div>
      <div className={styles.stats} aria-live="off">
        <strong className={styles.strong}>{Math.floor(fraction * 100)}%</strong>
        <span className={styles.sep}>·</span>
        <span>
          <strong className={styles.strong}>{formatCount(done)}</strong> / {formatCount(progress.total)} files
        </span>
        <span className={styles.sep}>·</span>
        <span>{formatDuration(progress.elapsedMs)} elapsed</span>
        {progress.remainingMs !== null && (
          <>
            <span className={styles.sep}>·</span>
            <span>about {formatDuration(progress.remainingMs)} left</span>
          </>
        )}
        {saved && (
          <>
            <span className={styles.sep}>·</span>
            <span className={saved.larger ? styles.warn : styles.ok}>{saved.text}</span>
          </>
        )}
        {progress.failed > 0 && (
          <>
            <span className={styles.sep}>·</span>
            <button type="button" className={`link ${styles.badLink}`} onClick={() => setFilter("failed")}>
              {formatCount(progress.failed)} failed
            </button>
          </>
        )}
      </div>
      {run.state === "running" && (
        <button type="button" className="button" onClick={() => void pause()}>
          Pause
        </button>
      )}
      {run.state === "paused" && (
        <button type="button" className="button" onClick={() => void resume()}>
          Resume
        </button>
      )}
      <button type="button" className="button" disabled={run.state === "cancelling"} onClick={() => void cancel()}>
        Cancel
      </button>
    </>
  );
}

function Finished({ run, plan }: { run: RunView; plan: PlanSummary | null }) {
  const convert = useSession((state) => state.convert);
  const clearInputs = useSession((state) => state.clearInputs);
  const setFilter = useSession((state) => state.setFilter);
  const { progress } = run;
  const saved = savedText(progress);
  const left = leftToConvert(run);

  return (
    <>
      <div className={styles.stats} aria-live="polite">
        <span className={styles.state}>Finished in {formatDuration(progress.elapsedMs)}</span>
        <span className={styles.sep}>·</span>
        <span>
          <strong className={styles.strong}>{formatCount(progress.converted)}</strong> written
        </span>
        {saved && (
          <>
            <span className={styles.sep}>·</span>
            <span className={saved.larger ? styles.warn : styles.ok}>{saved.text}</span>
          </>
        )}
        {plan && plan.skipped > 0 && (
          <>
            <span className={styles.sep}>·</span>
            <span>{formatCount(plan.skipped)} skipped</span>
          </>
        )}
        {progress.failed > 0 && (
          <>
            <span className={styles.sep}>·</span>
            <button type="button" className={`link ${styles.badLink}`} onClick={() => setFilter("failed")}>
              {formatCount(progress.failed)} failed
            </button>
          </>
        )}
        {progress.cancelled > 0 && (
          <>
            <span className={styles.sep}>·</span>
            <span>{formatCount(progress.cancelled)} cancelled</span>
          </>
        )}
      </div>
      <button type="button" className={`button ${styles.tallButton}`} onClick={() => void clearInputs()}>
        Clear list
      </button>
      <button
        type="button"
        className={`button button-primary ${styles.primary}`}
        disabled={left === 0}
        title={left === 0 ? "Everything was written or skipped" : "Plans again and converts what didn't get written"}
        onClick={() => void convert()}
      >
        {left > 0 ? `Convert ${formatCount(left)} left` : "Convert"}
      </button>
    </>
  );
}

/** One line along the bottom: ready, converting or finished. */
export function BottomBar({ overview }: { overview: Overview }) {
  const run = useSession((state) => state.run);
  return (
    <footer className={styles.bar}>
      {run === null && <Prepare overview={overview} />}
      {isRunActive(run) && run && <Running run={run} />}
      {run?.state === "finished" && <Finished run={run} plan={overview.plan} />}
    </footer>
  );
}

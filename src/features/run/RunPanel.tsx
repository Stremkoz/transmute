import type { Progress } from "@bindings";
import { formatBytes, formatCount, formatDuration, plural } from "../../lib/units";
import type { RunView } from "../../state/session";
import { useSession } from "../../state/sessionContext";
import styles from "./RunPanel.module.css";

const TITLES: Record<RunView["state"], string> = {
  running: "Converting",
  paused: "Paused",
  cancelling: "Cancelling…",
  finished: "Finished",
};

function finishedCount(progress: Progress): number {
  return progress.converted + progress.failed + progress.cancelled;
}

/** Sizes of converted outputs only, so the saving is a real number. */
function Savings({ progress }: { progress: Progress }) {
  if (progress.converted === 0 || progress.inputBytes === 0) return null;
  const change = 1 - progress.outputBytes / progress.inputBytes;
  const verdict =
    change >= 0 ? `${Math.round(change * 100)}% smaller` : `${Math.round(-change * 100)}% larger`;
  return (
    <p className={styles.savings}>
      {formatBytes(progress.inputBytes)} → {formatBytes(progress.outputBytes)} <span>({verdict})</span>
    </p>
  );
}

function Summary({ progress }: { progress: Progress }) {
  const parts = [`Converted ${plural(progress.converted, "file")} in ${formatDuration(progress.elapsedMs)}.`];
  if (progress.failed > 0) parts.push(`${plural(progress.failed, "file")} failed.`);
  if (progress.cancelled > 0) parts.push(`${formatCount(progress.cancelled)} cancelled.`);
  return <p className={styles.summary}>{parts.join(" ")}</p>;
}

/** The left column while a run is going, and its summary afterwards. */
export function RunPanel({ run }: { run: RunView }) {
  const pause = useSession((state) => state.pause);
  const resume = useSession((state) => state.resume);
  const cancel = useSession((state) => state.cancel);
  const closeRun = useSession((state) => state.closeRun);
  const { progress, state } = run;
  const done = finishedCount(progress);
  const fraction = progress.total > 0 ? done / progress.total : 1;

  return (
    <section className={styles.panel} aria-labelledby="run-heading">
      <h2 id="run-heading" className={styles.title}>
        {TITLES[state]}
      </h2>
      <div
        className={styles.bar}
        role="progressbar"
        aria-label="Conversion progress"
        aria-valuemin={0}
        aria-valuemax={progress.total}
        aria-valuenow={done}
      >
        <div className={styles.fill} style={{ width: `${(fraction * 100).toFixed(1)}%` }} />
      </div>
      <dl className={styles.stats}>
        <div>
          <dt>Converted</dt>
          <dd>{formatCount(progress.converted)}</dd>
        </div>
        <div>
          <dt>Failed</dt>
          <dd className={progress.failed > 0 ? styles.bad : undefined}>{formatCount(progress.failed)}</dd>
        </div>
        <div>
          <dt>{state === "finished" ? "Cancelled" : "Remaining"}</dt>
          <dd>{formatCount(state === "finished" ? progress.cancelled : progress.total - done)}</dd>
        </div>
        <div>
          <dt>Elapsed</dt>
          <dd>{formatDuration(progress.elapsedMs)}</dd>
        </div>
        {state !== "finished" && progress.remainingMs !== null && (
          <div>
            <dt>Time left</dt>
            <dd>about {formatDuration(progress.remainingMs)}</dd>
          </div>
        )}
      </dl>
      <Savings progress={progress} />
      {state === "finished" && <Summary progress={progress} />}
      <div className={styles.actions}>
        {state === "running" && (
          <button type="button" className="button" onClick={() => void pause()}>
            Pause
          </button>
        )}
        {state === "paused" && (
          <button type="button" className="button" onClick={() => void resume()}>
            Resume
          </button>
        )}
        {(state === "running" || state === "paused") && (
          <button type="button" className="button" onClick={() => void cancel()}>
            Cancel
          </button>
        )}
        {state === "finished" && (
          <button type="button" className="button" onClick={closeRun}>
            Back to inputs
          </button>
        )}
      </div>
      {state === "paused" && <p className={styles.hint}>Conversions already running finish; no new ones start.</p>}
    </section>
  );
}

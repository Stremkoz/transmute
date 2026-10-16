import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import styles from "./CompareViewer.module.css";
import { geometry, type Geometry, type Layout, type Pan, type Zoom } from "./geometry";

/** Size used before the stage is measured (and in tests, which have no layout). */
const UNMEASURED = { width: 960, height: 600 };

function useSize<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [size, setSize] = useState(UNMEASURED);
  useEffect(() => {
    const element = ref.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry && entry.contentRect.width > 0) {
        setSize({ width: entry.contentRect.width, height: entry.contentRect.height });
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return { ref, size };
}

interface StageProps {
  original: string;
  /** The output's image, or a message when there isn't one to show. */
  output: { url: string } | { message: ReactNode };
  image: { width: number; height: number };
  layout: Layout;
  zoom: Zoom;
  pan: Pan;
  onPan(pan: Pan): void;
  /** Showing the original in the output's place (Space held). */
  peeking: boolean;
  originalLabel: ReactNode;
  outputLabel: ReactNode;
}

function Picture({ url, place, alt }: { url: string; place: Geometry; alt: string }) {
  return (
    <img
      className={styles.picture}
      src={url}
      alt={alt}
      draggable={false}
      style={{ left: place.left, top: place.top, width: place.width, height: place.height }}
    />
  );
}

/** Drags pan the image when it's bigger than the stage; `onDivider` takes drags otherwise. */
function useDrag(place: Geometry, pan: Pan, onPan: (pan: Pan) => void, onDivider?: (fraction: number) => void) {
  const start = useRef<{ x: number; y: number; pan: Pan; divider: boolean } | null>(null);
  const pannable = place.slack.x > 0 || place.slack.y > 0;
  const dividerAt = (event: ReactPointerEvent<HTMLElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    if (box.width > 0) onDivider?.(Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)));
  };
  return {
    onPointerDown(event: ReactPointerEvent<HTMLElement>) {
      const onHandle = (event.target as HTMLElement).dataset.handle === "divider";
      const divider = onDivider !== undefined && (onHandle || !pannable);
      if (!divider && !pannable) return;
      event.currentTarget.setPointerCapture?.(event.pointerId);
      start.current = { x: event.clientX, y: event.clientY, pan, divider };
      if (divider) dividerAt(event);
    },
    onPointerMove(event: ReactPointerEvent<HTMLElement>) {
      const drag = start.current;
      if (!drag) return;
      if (drag.divider) dividerAt(event);
      else onPan({ x: drag.pan.x + event.clientX - drag.x, y: drag.pan.y + event.clientY - drag.y });
    },
    onPointerUp() {
      start.current = null;
    },
    pannable,
  };
}

function Message({ children }: { children: ReactNode }) {
  return <div className={styles.message}>{children}</div>;
}

/** The two images, laid over each other with a divider, or side by side. */
export function ImageStage(props: StageProps) {
  const { original, output, image, layout, zoom, pan, onPan, peeking, originalLabel, outputLabel } = props;
  const { ref, size } = useSize<HTMLDivElement>();
  const [divider, setDivider] = useState(0.5);
  const half = layout === "side" ? { width: (size.width - 12) / 2, height: size.height } : size;
  const place = geometry(half, image, zoom, pan);
  const { pannable, ...drag } = useDrag(place, pan, onPan, layout === "divider" ? setDivider : undefined);
  const outputUrl = "url" in output ? output.url : null;
  const shown = peeking || outputUrl === null ? original : outputUrl;

  if (layout === "side") {
    return (
      <div ref={ref} className={`${styles.stage} ${styles.side}`} {...drag} data-pannable={pannable}>
        <div className={styles.pane}>
          <Picture url={original} place={place} alt="Original" />
          <span className={`${styles.label} ${styles.labelLeft}`}>{originalLabel}</span>
        </div>
        <div className={styles.pane}>
          {outputUrl === null && !peeking && "message" in output ? (
            <Message>{output.message}</Message>
          ) : (
            <Picture url={shown} place={place} alt={peeking ? "Original" : "Output"} />
          )}
          <span className={`${styles.label} ${styles.labelLeft}`}>{peeking ? "Original" : outputLabel}</span>
        </div>
      </div>
    );
  }

  const split = `${(divider * 100).toFixed(2)}%`;
  return (
    <div ref={ref} className={styles.stage} {...drag} data-pannable={pannable}>
      <Picture url={original} place={place} alt="Original" />
      {outputUrl !== null && !peeking && (
        <div className={styles.clip} style={{ clipPath: `inset(0 0 0 ${split})` }}>
          <Picture url={outputUrl} place={place} alt="Output" />
        </div>
      )}
      {outputUrl === null && "message" in output && (
        <div className={styles.clip} style={{ clipPath: `inset(0 0 0 ${split})` }}>
          <Message>{output.message}</Message>
        </div>
      )}
      {!peeking && (
        <div
          className={styles.divider}
          style={{ left: split }}
          data-handle="divider"
          role="slider"
          aria-label="Divider"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(divider * 100)}
          tabIndex={0}
          onKeyDown={(event) => {
            if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
              event.stopPropagation();
              event.preventDefault();
              setDivider((d) => Math.min(1, Math.max(0, d + (event.key === "ArrowLeft" ? -0.05 : 0.05))));
            }
          }}
        >
          <span className={styles.knob} data-handle="divider" />
        </div>
      )}
      <span className={`${styles.label} ${styles.labelLeft}`}>{originalLabel}</span>
      {!peeking && <span className={`${styles.label} ${styles.labelRight}`}>{outputLabel}</span>}
    </div>
  );
}

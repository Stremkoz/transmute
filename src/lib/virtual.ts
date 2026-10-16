import { useEffect, useRef, useState, type UIEvent } from "react";

/** Rows drawn before the viewport has been measured (and in tests). */
const UNMEASURED_ROWS = 60;

/**
 * Windowing for a list of fixed-height rows: only the rows in view (plus a
 * margin) are mounted, so a 50,000-file list costs what a screenful does.
 * Put `ref` and `onScroll` on the scrolling element, give its content a
 * height of `height`, and draw rows `start` to `end` offset by `top`.
 */
export function useWindowedRows(count: number, rowHeight: number, overscan = 10) {
  const ref = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(0);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    const measure = () => setViewport(element.clientHeight);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const visible = viewport > 0 ? Math.ceil(viewport / rowHeight) : UNMEASURED_ROWS;
  const first = Math.floor(scrollTop / rowHeight);
  const start = Math.max(0, first - overscan);
  const end = Math.min(count, first + visible + overscan);
  return {
    ref,
    onScroll: (event: UIEvent<HTMLElement>) => setScrollTop(event.currentTarget.scrollTop),
    start,
    end,
    top: start * rowHeight,
    height: count * rowHeight,
  };
}

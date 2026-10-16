/** Where images sit in the compare viewer. */

export type Zoom = "fit" | 1 | 2;
export type Layout = "divider" | "side";
export interface Pan {
  x: number;
  y: number;
}

export interface Geometry {
  left: number;
  top: number;
  width: number;
  height: number;
  /** How far the image may be panned each way. */
  slack: Pan;
}

/** Where the image sits: fitted (never enlarged), or at 100% / 200% of its pixels. */
export function geometry(stage: { width: number; height: number }, image: { width: number; height: number }, zoom: Zoom, pan: Pan): Geometry {
  const pixel = typeof window === "undefined" ? 1 : window.devicePixelRatio || 1;
  const scale = zoom === "fit" ? Math.min(stage.width / image.width, stage.height / image.height, 1) : zoom / pixel;
  const width = image.width * scale;
  const height = image.height * scale;
  const slack = { x: Math.max(0, (width - stage.width) / 2), y: Math.max(0, (height - stage.height) / 2) };
  const x = Math.max(-slack.x, Math.min(slack.x, pan.x));
  const y = Math.max(-slack.y, Math.min(slack.y, pan.y));
  return { left: (stage.width - width) / 2 + x, top: (stage.height - height) / 2 + y, width, height, slack };
}


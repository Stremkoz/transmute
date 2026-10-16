import type { Format, ToolId, ToolInfo } from "@bindings";
import { FORMAT_LABELS } from "./formats";

/** How someone gets a tool, for the Tools dialog. */
export interface ToolSource {
  /** Ships with Transmute (in its tools folder). */
  bundled: boolean;
  /** A command to paste into a terminal, if there's one. */
  command?: string;
  url: string;
}

const JXL: ToolSource = { bundled: true, url: "https://github.com/libjxl/libjxl/releases" };
const WEBP: ToolSource = { bundled: true, url: "https://developers.google.com/speed/webp/download" };
const VIPS: ToolSource = { bundled: true, url: "https://www.libvips.org/install.html" };
const MAGICK: ToolSource = {
  bundled: false,
  command: "winget install ImageMagick.ImageMagick",
  url: "https://imagemagick.org/script/download.php#windows",
};
const FFMPEG: ToolSource = { bundled: false, command: "winget install Gyan.FFmpeg", url: "https://ffmpeg.org/download.html" };

export const TOOL_SOURCES: Record<ToolId, ToolSource> = {
  cjxl: JXL,
  djxl: JXL,
  cwebp: WEBP,
  dwebp: WEBP,
  gif2webp: WEBP,
  vips: VIPS,
  vipsheader: VIPS,
  magick: MAGICK,
  ffmpeg: FFMPEG,
  ffprobe: FFMPEG,
};

/** What a missing tool would add, said before it's installed (it can't be probed). */
export const TOOL_PURPOSE: Record<ToolId, string> = {
  cjxl: "Writes JPEG XL.",
  djxl: "Reads JPEG XL, and restores JPEGs packed into JPEG XL.",
  cwebp: "Writes WebP, quickly and with the most control.",
  dwebp: "Reads WebP.",
  gif2webp: "Turns animated GIFs into animated WebP.",
  vips: "Reads and writes most formats (JPEG, PNG, AVIF, TIFF, GIF and more), fast.",
  vipsheader: "Reads image details quickly.",
  magick: "Optional. Adds BMP, a JPEG XL reader and other fallbacks.",
  ffmpeg: "Optional. A fallback for animation and a few formats.",
  ffprobe: "Optional. Reads animation details.",
};

function formatList(formats: Format[]): string {
  return formats.map((format) => FORMAT_LABELS[format]).join(", ");
}

/** "Reads JPEG, PNG · writes WebP", from what the tool reported. */
export function toolAbilities(info: ToolInfo): string {
  const parts: string[] = [];
  if (info.reads.length > 0) parts.push(`reads ${formatList(info.reads)}`);
  if (info.writes.length > 0) parts.push(`writes ${formatList(info.writes)}`);
  const line = parts.join(" · ");
  return line.charAt(0).toUpperCase() + line.slice(1);
}

/** Formats only this tool writes, among the available ones: what's lost without it. */
export function onlyWriter(info: ToolInfo, tools: ToolInfo[]): Format[] {
  const others = new Set(
    tools.filter((t) => t.tool !== info.tool && t.status.status === "available").flatMap((t) => t.writes),
  );
  return info.writes.filter((format) => !others.has(format));
}

export const SOURCE_LABELS: Record<string, string> = {
  bundled: "comes with Transmute",
  path: "found on this computer",
  configured: "chosen by you",
};

/** Counts tools needing attention: broken ones, not optional ones left uninstalled. */
export function toolProblems(tools: ToolInfo[]): number {
  return tools.filter(
    (info) =>
      info.status.status === "unusable" ||
      (info.status.status === "available" && info.status.outdated) ||
      (info.status.status === "missing" && TOOL_SOURCES[info.tool].bundled),
  ).length;
}

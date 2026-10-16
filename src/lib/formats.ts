import type { CodecPatch, Format, OutputTarget, ToolId } from "@bindings";

export const FORMAT_LABELS: Record<Format, string> = {
  jpeg: "JPEG",
  png: "PNG",
  webp: "WebP",
  jxl: "JPEG XL",
  avif: "AVIF",
  heif: "HEIF",
  tiff: "TIFF",
  gif: "GIF",
  bmp: "BMP",
};

export const TOOL_LABELS: Record<ToolId, string> = {
  cjxl: "cjxl (JPEG XL encoder)",
  djxl: "djxl (JPEG XL decoder)",
  cwebp: "cwebp (WebP encoder)",
  dwebp: "dwebp (WebP decoder)",
  gif2webp: "gif2webp (animated WebP)",
  vips: "libvips",
  vipsheader: "vipsheader (libvips)",
  magick: "ImageMagick",
  ffmpeg: "FFmpeg",
  ffprobe: "ffprobe (FFmpeg)",
};

/** True when a patch sets nothing beyond its format. */
function isDefaultPatch(codec: CodecPatch): boolean {
  return Object.keys(codec).every((key) => key === "format");
}

/**
 * A one-line description of an output for its card. Codec-specific
 * summaries (e.g. "q85 · method 4") arrive with the codec panels in G3.
 */
export function outputSummary(target: OutputTarget): string {
  return isDefaultPatch(target.codec) ? "Default settings" : "Custom settings";
}

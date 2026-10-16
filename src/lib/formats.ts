import type { Format, ToolId, ToolInfo } from "@bindings";

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

/** Output formats in the order the picker offers them: modern first. */
export const OUTPUT_FORMATS: Format[] = ["jxl", "webp", "avif", "jpeg", "png", "heif", "tiff", "gif", "bmp"];

/** Formats some available tool can write, from the backend's tool report. */
export function writableFormats(tools: ToolInfo[]): Set<Format> {
  return new Set(tools.filter((info) => info.status.status === "available").flatMap((info) => info.writes));
}

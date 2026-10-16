import type { CodecDefaults, CodecPatch, CodecSettings, Format, LosslessChoice, OutputTarget } from "@bindings";

/**
 * What each codec's settings are, for the codec drawer and the output
 * cards. Values (factory and inherited defaults) always come from the
 * backend (`codecs()`); this file only describes how to show them.
 */

/** A settings object of any codec, addressed by field name. */
export type Fields = Record<string, unknown>;

interface FieldBase {
  key: string;
  label: string;
  help: string;
  /** Hidden when it doesn't apply (quality in lossless mode, say). */
  shown?: (settings: Fields) => boolean;
}

export type FieldSpec = FieldBase &
  (
    /** Lossy or lossless; with `autoKey`, also Auto (that field on). */
    | { kind: "mode"; autoKey?: string }
    | { kind: "bool" }
    | { kind: "number"; min: number; max: number; step: number; marks?: [number, string][] }
    /** A number, or absent for "Automatic". */
    | { kind: "auto"; min: number; max: number; autoHelp: string }
    | { kind: "choice"; options: { value: string; label: string }[] }
    | { kind: "colour" }
  );

const lossy = (settings: Fields) => settings.lossless !== true;
const lossless = (settings: Fields) => settings.lossless === true;

const MODE: FieldSpec = {
  key: "lossless",
  label: "Mode",
  kind: "mode",
  help: "Lossy is far smaller and looks the same at normal sizes. Lossless keeps every pixel exactly.",
};

/** The share a lossless copy may be larger and still be kept (the backend's rule). */
const AUTO_LOSSLESS_GROWTH = "25%";

const WEBP_MODE: FieldSpec = {
  key: "lossless",
  label: "Mode",
  kind: "mode",
  autoKey: "autoLossless",
  help: `Lossy is far smaller and looks the same at normal sizes. Lossless keeps every pixel exactly. Auto also tries lossless on graphics (PNG, GIF, BMP, TIFF) and keeps it when it's at most ${AUTO_LOSSLESS_GROWTH} larger; photos stay lossy. Graphics take about twice as long.`,
};

export type Mode = "lossy" | "lossless" | "auto";

/** Which mode the settings are in. */
export function modeOf(settings: Fields, spec: FieldSpec): Mode {
  if (settings.lossless === true) return "lossless";
  return spec.kind === "mode" && spec.autoKey && settings[spec.autoKey] === true ? "auto" : "lossy";
}

export const CODEC_FIELDS: Record<Format, FieldSpec[]> = {
  jpeg: [
    {
      key: "quality",
      label: "Quality",
      kind: "number",
      min: 1,
      max: 100,
      step: 1,
      marks: [
        [60, "small"],
        [85, "very high"],
        [95, "near-original"],
      ],
      help: "Higher keeps more detail and makes bigger files. Above 90 files grow quickly for little visible gain.",
    },
    {
      key: "progressive",
      label: "Progressive",
      kind: "bool",
      help: "Usually a little smaller, and loads gradually on the web. Every modern viewer opens it.",
    },
    {
      key: "background",
      label: "Background",
      kind: "colour",
      help: "JPEG can't be transparent, so transparent areas are filled with this colour.",
    },
  ],
  png: [
    {
      key: "compression",
      label: "Compression",
      kind: "number",
      min: 0,
      max: 9,
      step: 1,
      help: "Always lossless: higher only makes smaller files, more slowly. 9 is 10–14% smaller than 6.",
    },
  ],
  webp: [
    WEBP_MODE,
    {
      key: "quality",
      label: "Quality",
      kind: "number",
      min: 0,
      max: 100,
      step: 1,
      marks: [
        [75, "small"],
        [85, "very high"],
        [95, "near-original"],
      ],
      help: "Higher keeps more detail and makes bigger files.",
      shown: lossy,
    },
    {
      key: "method",
      label: "Method",
      kind: "number",
      min: 0,
      max: 6,
      step: 1,
      help: "Higher is a little smaller but slower. 6 saves about 5% at up to 14× the time on graphics.",
    },
  ],
  jxl: [
    MODE,
    {
      key: "distance",
      label: "Distance",
      kind: "number",
      min: 0,
      max: 25,
      step: 0.1,
      marks: [
        [0, "lossless"],
        [1, "near-identical"],
        [3, "smaller"],
        [6, "smallest"],
      ],
      help: "How far the result may drift from the original. Around 1 is indistinguishable for most photos; each step up saves space and loses detail. 0 switches to lossless.",
      shown: lossy,
    },
    {
      key: "effort",
      label: "Effort",
      kind: "auto",
      min: 1,
      max: 10,
      help: "Higher is a little smaller but much slower.",
      autoHelp: "Automatic uses 5 for lossy images without transparency and 7 for everything else.",
    },
    {
      key: "reconstructJpeg",
      label: "Keep JPEGs exactly",
      kind: "bool",
      help: "JPEG sources are repacked so the original file can be rebuilt bit for bit, usually about 20% smaller.",
      shown: lossless,
    },
  ],
  avif: [
    MODE,
    {
      key: "quality",
      label: "Quality",
      kind: "number",
      min: 1,
      max: 100,
      step: 1,
      marks: [
        [60, "small"],
        [70, "very high"],
        [85, "near-original"],
      ],
      help: "Higher keeps more detail and makes bigger files. AVIF's scale runs lower than JPEG's: 70 here looks like JPEG 85.",
      shown: lossy,
    },
    {
      key: "effort",
      label: "Effort",
      kind: "number",
      min: 0,
      max: 9,
      step: 1,
      help: "Higher is a little smaller but much slower. Beyond 4 files barely shrink while encoding gets 2–3× slower.",
    },
  ],
  heif: [
    MODE,
    {
      key: "quality",
      label: "Quality",
      kind: "number",
      min: 1,
      max: 100,
      step: 1,
      marks: [
        [40, "small"],
        [50, "very high"],
        [60, "near-original"],
      ],
      help: "Higher keeps more detail and makes bigger files. HEIF's scale climbs fast: 50 looks like JPEG 85, and 60 is already 75% bigger.",
      shown: lossy,
    },
  ],
  tiff: [
    {
      key: "compression",
      label: "Compression",
      kind: "choice",
      options: [
        { value: "lzw", label: "LZW" },
        { value: "deflate", label: "Deflate" },
        { value: "zstd", label: "Zstandard" },
        { value: "none", label: "None" },
      ],
      help: "All lossless. LZW opens in every program; Deflate and Zstandard are smaller but less widely supported.",
    },
  ],
  gif: [],
  bmp: [],
};

/** One line per format for the Add output menu. */
export const FORMAT_DESCRIPTIONS: Record<Format, string> = {
  jxl: "Smallest at high quality, with true lossless and exact JPEG repacking. Newer viewers only.",
  webp: "Small, and opens in every browser.",
  avif: "Very small for photos and graphics; slower to make.",
  jpeg: "Opens everywhere. No transparency.",
  png: "Lossless, and opens everywhere. Large for photos.",
  heif: "Apple's photo format. Small, but little support outside Apple.",
  tiff: "Lossless, for print and editing software.",
  gif: "Simple graphics and animation, 256 colours.",
  bmp: "Uncompressed. Only for old software.",
};

/** Why a format has no controls, for the drawer. */
export const NO_SETTINGS: Partial<Record<Format, string>> = {
  gif: "GIF has no settings: its palette is chosen automatically for each image.",
  bmp: "BMP has no settings: it's stored uncompressed.",
};

export function defaultsFor(codecs: CodecDefaults[], format: Format): CodecDefaults | undefined {
  return codecs.find((codec) => codec.format === format);
}

/** The settings an output ends up with: its patch over what it inherits. */
export function effective(inherited: CodecSettings, patch: CodecPatch): Fields {
  const settings: Fields = { ...inherited, ...patch };
  // As the backend does: distance 0 is lossless.
  if (settings.format === "jxl" && settings.distance === 0) {
    settings.lossless = true;
    settings.distance = (inherited as Fields).distance;
  }
  return settings;
}

/** True when the patch sets this field itself. */
export function isSet(patch: CodecPatch, key: string): boolean {
  const fields = patch as Fields;
  // WebP's mode is two fields.
  if (key === "lossless" && fields.autoLossless !== undefined) return true;
  return fields[key] !== undefined;
}

/** The patch with one field set, or removed (inherited) when `value` is undefined. */
export function withField(patch: CodecPatch, key: string, value: unknown): CodecPatch {
  const next: Fields = { ...patch };
  if (value === undefined) delete next[key];
  else next[key] = value;
  // Distance 0 means lossless: say so explicitly, keeping a usable distance.
  if (next.format === "jxl" && key === "distance" && value === 0) {
    delete next.distance;
    next.lossless = true;
  }
  // WebP's mode sets both fields, so a mode inherited as Auto can be left.
  if (next.format === "webp" && key === "lossless") {
    // Lossless overrides Auto, so only Lossy needs to turn it off.
    if (value === undefined || value === true) delete next.autoLossless;
    else {
      next.lossless = false;
      next.autoLossless = value === "auto";
    }
  }
  return next as CodecPatch;
}

/** True when the patch sets nothing beyond its format. */
export function isDefaultPatch(codec: CodecPatch): boolean {
  return Object.keys(codec).every((key) => key === "format" || (codec as Fields)[key] === undefined);
}

function trimNumber(value: unknown): string {
  return typeof value === "number" ? String(Math.round(value * 100) / 100) : String(value);
}

/** What an output's settings are, in one line for its card. */
export function settingsSummary(settings: Fields): string {
  const mode = settings.lossless === true ? "Lossless" : "Lossy";
  const effort = (value: unknown) => (value === undefined || value === null ? "effort automatic" : `effort ${trimNumber(value)}`);
  switch (settings.format as Format) {
    case "jpeg": {
      const parts = [`Quality ${trimNumber(settings.quality)}`];
      if (settings.progressive === true) parts.push("progressive");
      if (typeof settings.background === "string" && settings.background.toLowerCase() !== "#ffffff") {
        parts.push(`background ${settings.background}`);
      }
      return parts.join(" · ");
    }
    case "png":
      return `Lossless · compression ${trimNumber(settings.compression)}`;
    case "webp":
      if (settings.lossless === true) return `Lossless · method ${trimNumber(settings.method)}`;
      return `${settings.autoLossless === true ? "Auto lossless" : "Lossy"} · quality ${trimNumber(settings.quality)} · method ${trimNumber(settings.method)}`;
    case "jxl":
      return settings.lossless === true
        ? [mode, effort(settings.effort), ...(settings.reconstructJpeg === true ? ["keep JPEGs exactly"] : [])].join(" · ")
        : `${mode} · distance ${trimNumber(settings.distance)} · ${effort(settings.effort)}`;
    case "avif":
      return settings.lossless === true
        ? `Lossless · effort ${trimNumber(settings.effort)}`
        : `Lossy · quality ${trimNumber(settings.quality)} · effort ${trimNumber(settings.effort)}`;
    case "heif":
      return settings.lossless === true ? "Lossless" : `Lossy · quality ${trimNumber(settings.quality)}`;
    case "tiff": {
      const option = CODEC_FIELDS.tiff[0];
      const label =
        option?.kind === "choice" ? option.options.find((o) => o.value === settings.compression)?.label : undefined;
      return `Lossless · ${label ?? String(settings.compression)} compression`;
    }
    case "gif":
    case "bmp":
      return "No settings";
  }
}

/** Notes on what an output does differently from the others. */
export function overrideNotes(target: OutputTarget): string[] {
  const notes: string[] = [];
  if (target.destination) notes.push("own folder");
  if (target.naming) notes.push("own file names");
  if (target.metadata) notes.push("own metadata");
  return notes;
}

/** The settings an image was actually encoded with, once Auto has chosen. */
export function chosenSettings(settings: Fields, choice: LosslessChoice | null | undefined): Fields {
  return choice ? { ...settings, lossless: choice.keptLossless, autoLossless: false } : settings;
}

/** What Auto found, in a few words: "Auto kept lossless: 6% larger than lossy". */
export function losslessChoiceText(choice: LosslessChoice): string {
  const { keptLossless, lossyBytes, losslessBytes } = choice;
  if (!keptLossless) return `Auto kept lossy: lossless was ${(losslessBytes / Math.max(lossyBytes, 1)).toFixed(1)}× the size`;
  const change = Math.round((losslessBytes / Math.max(lossyBytes, 1) - 1) * 100);
  if (change > 0) return `Auto kept lossless: ${change}% larger than lossy`;
  return change < 0 ? `Auto kept lossless: ${-change}% smaller than lossy` : "Auto kept lossless: the same size as lossy";
}

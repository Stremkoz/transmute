import type { CollisionPolicy, DateStyle, Destination, Format, MetadataPolicy, NamingRule } from "@bindings";

/**
 * Words for the "Where and how" settings (docs/preset-format.md), shared
 * by the outputs column, the codec drawer and the Settings dialog.
 */

export const FACTORY_DESTINATION: Destination = { mode: "besideSource" };
export const FACTORY_COLLISION: CollisionPolicy = "skip";
export const FACTORY_METADATA: MetadataPolicy = "preserveAll";

export const EXTENSIONS: Record<Format, string> = {
  jpeg: "jpg",
  png: "png",
  webp: "webp",
  jxl: "jxl",
  avif: "avif",
  heif: "heic",
  tiff: "tiff",
  gif: "gif",
  bmp: "bmp",
};

export function destinationLabel(destination: Destination): string {
  switch (destination.mode) {
    case "besideSource":
      return "Beside the originals";
    case "subfolder":
      return "A subfolder per format";
    case "custom":
      return destination.perFormatSubfolders ? `${destination.root}, a folder per format` : destination.root;
  }
}

export const COLLISION_OPTIONS: { value: CollisionPolicy; label: string; help: string }[] = [
  { value: "skip", label: "Skip it", help: "Leave the existing file and don't convert that one." },
  { value: "rename", label: "Keep both", help: "Write the new file under a different name." },
  { value: "overwrite", label: "Replace it", help: "Replace the existing file. Originals are never replaced." },
];

export const METADATA_OPTIONS: { value: MetadataPolicy; label: string; help: string }[] = [
  {
    value: "preserveAll",
    label: "Keep all",
    help: "Camera details, location, dates and the colour profile, as in the original.",
  },
  {
    value: "copyrightOnly",
    label: "Copyright only",
    help: "Only the copyright and artist. Location, camera and dates are removed.",
  },
  {
    value: "colorProfileOnly",
    label: "Colour profile only",
    help: "Removes everything except the colour profile, so colours still look right.",
  },
  { value: "stripAll", label: "Remove all", help: "Removes everything, including the colour profile." },
];

export function collisionLabel(policy: CollisionPolicy): string {
  return COLLISION_OPTIONS.find((option) => option.value === policy)?.label ?? policy;
}

export function metadataLabel(policy: MetadataPolicy): string {
  return METADATA_OPTIONS.find((option) => option.value === policy)?.label ?? policy;
}

export const NAME_TOKENS: { token: string; label: string }[] = [
  { token: "{date}", label: "Date" },
  { token: "{counter}", label: "Number" },
  { token: "{format}", label: "Format" },
  { token: "{original_ext}", label: "Original extension" },
];

export const DATE_STYLES: { value: DateStyle; label: string }[] = [
  { value: "compact", label: "20261015" },
  { value: "short", label: "261015" },
  { value: "iso", label: "2026-10-15" },
];

/** Characters Windows forbids in file names (the backend rejects them too). */
const FORBIDDEN = /[<>:"/\\|?*]/;

/** Why a prefix or suffix can't be used, or null if it can. */
export function namePartProblem(text: string): string | null {
  if (FORBIDDEN.test(text)) return 'File names can\'t contain < > : " / \\ | ? or *';
  const unescaped = text.replaceAll("{{", "").replaceAll("}}", "");
  for (const match of unescaped.matchAll(/\{([^{}]*)\}/g)) {
    if (!NAME_TOKENS.some((t) => t.token === match[0])) return `Unknown token ${match[0]}`;
  }
  if (/[{}]/.test(unescaped.replace(/\{[^{}]*\}/g, ""))) {
    return "Braces hold a token, like {date}. Write {{ or }} for a brace itself.";
  }
  return null;
}

export function isIdentityNaming(rule: NamingRule | undefined): boolean {
  return !rule || (rule.prefix === "" && rule.suffix === "");
}

function sampleDate(style: DateStyle): string {
  return DATE_STYLES.find((d) => d.value === style)?.label ?? "20261015";
}

/** A prefix or suffix with its tokens filled in with sample values. */
function expand(text: string, rule: NamingRule, format: Format, originalExt: string): string {
  return text
    .replaceAll("{{", "\u0000")
    .replaceAll("}}", "\u0001")
    .replaceAll("{date}", sampleDate(rule.dateStyle))
    .replaceAll("{counter}", "001")
    .replaceAll("{format}", EXTENSIONS[format])
    .replaceAll("{original_ext}", originalExt)
    .replaceAll("\u0000", "{")
    .replaceAll("\u0001", "}");
}

/** What a file called `name.originalExt` becomes under `rule`. */
export function previewName(rule: NamingRule | undefined, name: string, originalExt: string, format: Format): string {
  const ext = EXTENSIONS[format];
  if (!rule) return `${name}.${ext}`;
  return `${expand(rule.prefix, rule, format, originalExt)}${name}${expand(rule.suffix, rule, format, originalExt)}.${ext}`;
}

export function namingLabel(rule: NamingRule | undefined): string {
  if (!rule || isIdentityNaming(rule)) return "Unchanged";
  return `${rule.prefix}‹name›${rule.suffix}`;
}

export const EMPTY_NAMING: NamingRule = { prefix: "", suffix: "", dateStyle: "compact", dateSource: "batchStart" };

/** The last `count` components of a path, for showing long paths briefly. */
export function pathTail(path: string, count = 2): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  const tail = parts.slice(-count).join("\\");
  return parts.length > count ? `…\\${tail}` : tail;
}

/** A file's name without its extension, and the extension. */
export function splitName(path: string): { stem: string; ext: string } {
  const name = path.split(/[\\/]/).pop() ?? path;
  const dot = name.lastIndexOf(".");
  return dot > 0 ? { stem: name.slice(0, dot), ext: name.slice(dot + 1) } : { stem: name, ext: "" };
}

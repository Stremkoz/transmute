import type {
  ApiError,
  ApiEvent,
  AppSettings,
  CodecDefaults,
  CodecPatch,
  CodecSettings,
  EntryDetails,
  ErrorCode,
  FileCounts,
  FileFilter,
  FileInfo,
  FileRow,
  Format,
  InputsSummary,
  Job,
  JobState,
  JobUpdate,
  NamingRule,
  Overview,
  PlanSummary,
  Preset,
  Preview,
  Progress,
  RunStatus,
  Sample,
  StoredPreset,
  Tally,
  TargetTotals,
  ToolInfo,
  TreeNode,
  UserDefaults,
} from "@bindings";
import { writableFormats } from "../lib/formats";
import type { TransmuteApi } from "./client";

/**
 * An in-memory stand-in for `transmute-api`, so the UI runs in a plain
 * browser (`npm run dev`) and in tests without the Tauri shell.
 *
 * It mirrors the real session's rules where the UI depends on them: a plan
 * is invalidated by any edit, a run needs a fresh plan, edits are refused
 * while running, files are counted in every state that applies, and
 * clearing finished files keeps failures. Everything else is faked
 * plausibly. Commands it doesn't fake yet reject with an `internal` error.
 */
export interface MockApi extends TransmuteApi {
  /** Pushes an event to every listener, as the shell would. */
  emit(event: ApiEvent): void;
}

export interface MockSeed {
  preset?: Preset;
  /** Stored presets, by id. */
  presets?: Record<string, Preset>;
  defaults?: UserDefaults;
  tools?: ToolInfo[];
  notices?: string[];
  /** Milliseconds between simulated run steps. */
  runStepMs?: number;
  /** Every nth job fails (0: none). */
  failEvery?: number;
}

/** A first launch: nothing picked yet. */
export const MOCK_PRESET: Preset = {
  name: "Untitled",
  inputFilter: { mode: "all" },
  outputs: [],
};

export const MOCK_TOOLS: ToolInfo[] = [
  available("cjxl", "0.11.1", ["png", "jpeg", "gif"], ["jxl"]),
  available("djxl", "0.11.1", ["jxl"], ["png"]),
  available("cwebp", "1.5.0", ["png", "jpeg", "tiff", "webp"], ["webp"]),
  available("dwebp", "1.5.0", ["webp"], ["png"]),
  available("gif2webp", "1.5.0", ["gif"], ["webp"]),
  available("vips", "8.17.2", ["jpeg", "png", "webp", "avif", "heif", "tiff", "gif", "jxl"], [
    "jpeg",
    "png",
    "webp",
    "avif",
    "tiff",
    "gif",
  ]),
  available("vipsheader", "8.17.2", [], []),
  available("magick", "7.1.2-3", ["jpeg", "png", "webp", "tiff", "gif", "bmp"], [
    "jpeg",
    "png",
    "webp",
    "tiff",
    "gif",
    "bmp",
  ]),
  { tool: "ffmpeg", status: { status: "missing" }, reads: [], writes: [], notes: [] },
  { tool: "ffprobe", status: { status: "missing" }, reads: [], writes: [], notes: [] },
];

function available(
  tool: ToolInfo["tool"],
  version: string,
  reads: ToolInfo["reads"],
  writes: ToolInfo["writes"],
): ToolInfo {
  return {
    tool,
    status: { status: "available", path: `C:\\Tools\\${tool}.exe`, version, source: "path", outdated: false },
    reads,
    writes,
    notes: [],
  };
}

/**
 * The backend's factory defaults (transmute-core/src/codec.rs), so the mock
 * can answer `codecs()`. Only the mock repeats them; the UI always asks.
 */
export const MOCK_FACTORY: CodecSettings[] = [
  { format: "jpeg", quality: 85, progressive: true, background: "#ffffff" },
  { format: "png", compression: 9 },
  { format: "webp", lossless: false, quality: 85, method: 4 },
  { format: "jxl", lossless: false, distance: 1.5, reconstructJpeg: true },
  { format: "avif", lossless: false, quality: 70, effort: 4 },
  { format: "heif", lossless: false, quality: 50 },
  { format: "tiff", compression: "lzw" },
  { format: "gif" },
  { format: "bmp" },
];

/**
 * Documents as the real backend sends them: serde leaves out an input
 * filter of "all", empty codec defaults and empty tool paths (G5: the
 * frontend crashed on exactly these, which the mock used to always send).
 */
function wirePreset(preset: Preset): Preset {
  const out = copy(preset);
  if (out.inputFilter?.mode === "all") delete out.inputFilter;
  wireNaming(out.naming);
  for (const output of out.outputs) wireNaming(output.naming);
  return out;
}

function wireDefaults(defaults: UserDefaults): UserDefaults {
  const out = copy(defaults);
  if (out.codecs?.length === 0) delete out.codecs;
  wireNaming(out.naming);
  return out;
}

function wireNaming(rule: NamingRule | undefined) {
  if (rule?.prefix === "") delete rule.prefix;
  if (rule?.suffix === "") delete rule.suffix;
}

function wireSettings(settings: AppSettings): AppSettings {
  const out = copy(settings);
  if (out.toolPaths && Object.keys(out.toolPaths).length === 0) delete out.toolPaths;
  return out;
}

/** Deep equality of JSON-like values, ignoring absent and undefined fields. */
function sameValue(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  if (Array.isArray(a) !== Array.isArray(b)) return false;
  const record = (value: object) =>
    Object.fromEntries(Object.entries(value).filter(([, v]) => v !== undefined)) as Record<string, unknown>;
  const left = record(a);
  const right = record(b);
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  return [...keys].every((key) => sameValue(left[key], right[key]));
}

const EXTENSIONS: Record<string, Format> = {
  jpg: "jpeg",
  jpeg: "jpeg",
  png: "png",
  webp: "webp",
  jxl: "jxl",
  avif: "avif",
  heic: "heif",
  heif: "heif",
  tif: "tiff",
  tiff: "tiff",
  gif: "gif",
  bmp: "bmp",
};

const DESCRIPTIONS: Record<Format, string> = {
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

const EXTENSION_OF: Record<Format, string> = {
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

/** Formats in a made-up folder, in turn. */
const FOLDER_FORMATS: Format[] = ["jpeg", "jpeg", "jpeg", "png", "png", "webp", "heif", "jpeg"];
/** Subfolders of a made-up folder; "" is its top level. */
const SUBFOLDERS = ["", "Day 1", "Day 2"];

interface MockEntry {
  id: number;
  root: number;
  /** `/`-separated, relative to the root. */
  relativePath: string;
  format: Format | null;
  description: string;
  size: number;
  width: number | null;
  height: number | null;
  excluded: boolean;
}

interface MockRoot {
  id: number;
  path: string;
  isFolder: boolean;
}

interface MockJob {
  id: number;
  entry: number;
  target: number;
  format: Format;
  output: string;
  state: JobState;
}

interface MockPlan {
  summary: PlanSummary;
  jobs: MockJob[];
  byEntry: Map<number, MockJob[]>;
  used: boolean;
}

interface MockRun {
  state: RunStatus["state"];
  progress: Progress;
  timer: ReturnType<typeof setInterval>;
}

function failure(code: ErrorCode, message: string): ApiError {
  return { code, message };
}

function notYet(command: string): Promise<never> {
  return Promise.reject(failure("internal", `the mock backend doesn't fake ${command} yet`));
}

/** Deep copy, so callers can't mutate the mock's state through results. */
function copy<T>(value: T): T {
  return structuredClone(value);
}

/** A stable small number from a string, so made-up folders don't change. */
function hash(text: string): number {
  let value = 0;
  for (const char of text) value = (value * 31 + char.charCodeAt(0)) >>> 0;
  return value;
}

function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

/**
 * A made-up picture for a file: a landscape in colours from its name, so
 * the browser build has something to compare. `softness` blurs it a little,
 * standing in for a lossy output.
 */
function picture(name: string, width: number, height: number, softness = 0): string {
  const hue = hash(name) % 360;
  const blur = softness > 0 ? `<filter id="b"><feGaussianBlur stdDeviation="${softness}"/></filter>` : "";
  const filter = softness > 0 ? ' filter="url(#b)"' : "";
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 160 90">${blur}<g${filter}><rect width="160" height="90" fill="hsl(${hue} 55% 62%)"/><circle cx="118" cy="24" r="11" fill="hsl(${(hue + 40) % 360} 90% 82%)"/><path d="M0 70 L38 34 L62 56 L92 22 L160 72 V90 H0Z" fill="hsl(${(hue + 180) % 360} 30% 30%)"/><path d="M0 80 Q40 66 80 78 T160 76 V90 H0Z" fill="hsl(${(hue + 200) % 360} 35% 20%)"/></g></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

function message(code: string, text: string) {
  return { code, text };
}

function emptyCounts(): FileCounts {
  return { all: 0, ready: 0, problems: 0, excluded: 0, running: 0, done: 0, failed: 0, cancelled: 0, unsupported: 0 };
}

/** Which job-dependent states a file is in (as `transmute-api` decides). */
function jobFlags(states: JobState[]) {
  let converted = false;
  let unfinished = false;
  const flags = { ready: false, problems: false, running: false, done: false, failed: false, cancelled: false };
  for (const { state } of states) {
    if (state === "ready") flags.ready = unfinished = true;
    else if (state === "skipped") flags.problems = true;
    else if (state === "blocked") flags.problems = unfinished = true;
    else if (state === "running") flags.running = unfinished = true;
    else if (state === "converted") converted = true;
    else if (state === "failed") flags.failed = unfinished = true;
    else if (state === "cancelled") flags.cancelled = unfinished = true;
  }
  flags.done = converted && !unfinished;
  return flags;
}

export function createMockApi(seed: MockSeed = {}): MockApi {
  const listeners = new Set<(event: ApiEvent) => void>();
  const presets = new Map<string, Preset>(Object.entries(copy(seed.presets ?? {})));
  // The stored preset the working one came from, as stored.
  let loaded: { id: string; stored: Preset } | null = null;
  const runStepMs = seed.runStepMs ?? 100;
  const failEvery = seed.failEvery ?? 50;
  let preset = copy(seed.preset ?? MOCK_PRESET);
  let tools = copy(seed.tools ?? MOCK_TOOLS);
  let notices = [...(seed.notices ?? [])];
  let defaults: UserDefaults = copy(seed.defaults ?? { codecs: [] });
  let settings: AppSettings = { toolPaths: {} };
  let roots: MockRoot[] = [];
  let entries: MockEntry[] = [];
  let nextRoot = 0;
  let nextEntry = 0;
  let plan: MockPlan | null = null;
  let run: MockRun | null = null;
  // Outputs earlier runs wrote (entry:target): the next plan skips them.
  const written = new Set<string>();

  const emit = (event: ApiEvent) => listeners.forEach((listener) => listener(event));

  const ensureIdle = () => {
    if (run && run.state !== "finished") {
      throw failure("busy", "a conversion is running; wait for it to finish or cancel it");
    }
  };

  /** Any edit invalidates the plan and the run, as in the real session. */
  const changed = () => {
    plan = null;
    run = null;
  };

  const addRoot = (path: string) => {
    const root = nextRoot++;
    const extension = /\.([a-z0-9]+)$/i.exec(path)?.[1]?.toLowerCase();
    const add = (relativePath: string, format: Format | null, index: number) =>
      entries.push({
        id: nextEntry++,
        root,
        relativePath,
        format,
        description: format ? DESCRIPTIONS[format] : "Text",
        size: 400_000 + ((hash(relativePath) + index * 7919) % 4_000_000),
        width: format ? 1920 + (index % 4) * 640 : null,
        height: format ? 1080 + (index % 3) * 360 : null,
        excluded: false,
      });
    if (extension !== undefined) {
      roots.push({ id: root, path, isFolder: false });
      add(baseName(path), EXTENSIONS[extension] ?? null, 0);
      return;
    }
    roots.push({ id: root, path, isFolder: true });
    const images = 12 + (hash(path) % 30);
    for (let i = 0; i < images; i++) {
      const format = FOLDER_FORMATS[i % FOLDER_FORMATS.length] ?? "jpeg";
      const folder = SUBFOLDERS[i % SUBFOLDERS.length] ?? "";
      const name = `photo-${String(i + 1).padStart(3, "0")}.${EXTENSION_OF[format]}`;
      add(folder ? `${folder}/${name}` : name, format, i);
    }
    add("notes.txt", null, images);
    entries.sort((a, b) => a.root - b.root || a.relativePath.localeCompare(b.relativePath));
  };

  const isImage = (entry: MockEntry) => entry.format !== null;

  const fileInfo = (entry: MockEntry): FileInfo => ({
    entry: entry.id,
    format: entry.format,
    description: entry.description,
    size: entry.size,
    width: entry.width,
    height: entry.height,
    excluded: entry.excluded,
    animated: false,
    problem: null,
    outputs: entry.excluded
      ? []
      : (plan?.byEntry.get(entry.id) ?? []).map((job) => ({
          job: job.id,
          target: job.target,
          format: job.format,
          state: copy(job.state),
        })),
  });

  const flagsOf = (entry: MockEntry) =>
    jobFlags(entry.excluded || !isImage(entry) ? [] : (plan?.byEntry.get(entry.id) ?? []).map((j) => j.state));

  const addTo = (counts: FileCounts, entry: MockEntry) => {
    counts.all++;
    if (entry.excluded) counts.excluded++;
    else if (!isImage(entry)) counts.unsupported++;
    else {
      const flags = flagsOf(entry);
      for (const key of ["ready", "problems", "running", "done", "failed", "cancelled"] as const) {
        if (flags[key]) counts[key]++;
      }
    }
  };

  const countsOf = (filter?: (entry: MockEntry) => boolean) => {
    const counts = emptyCounts();
    for (const entry of entries) if (!filter || filter(entry)) addTo(counts, entry);
    return counts;
  };

  const matches = (filter: FileFilter, entry: MockEntry) => {
    if (filter === "all") return true;
    if (filter === "excluded") return entry.excluded;
    if (filter === "unsupported") return !entry.excluded && !isImage(entry);
    return flagsOf(entry)[filter];
  };

  const summarize = (): InputsSummary => {
    const byFormat = new Map<Format, number>();
    let convertible = 0;
    let excluded = 0;
    for (const entry of entries) {
      if (entry.excluded) excluded++;
      if (entry.format) {
        convertible++;
        byFormat.set(entry.format, (byFormat.get(entry.format) ?? 0) + 1);
      }
    }
    return {
      roots: roots.map((root, index) => ({
        id: index,
        path: root.path,
        isFolder: root.isFolder,
        files: entries.filter((e) => e.root === root.id).length,
        counts: countsOf((e) => e.root === root.id),
      })),
      files: entries.length,
      convertible,
      excluded,
      unsupported: entries.length - convertible,
      byFormat: [...byFormat].map(([format, count]) => ({ format, count })),
      warnings: [],
    };
  };

  /** Roots are addressed by position, as the real API does. */
  const rootAt = (index: number) => {
    const root = roots[index];
    if (!root) throw failure("notFound", `no input ${index}`);
    return root;
  };

  const overview = (): Overview => ({
    inputs: summarize(),
    preset: wirePreset(preset),
    loaded: loaded
      ? { id: loaded.id, name: loaded.stored.name, edited: !sameValue(loaded.stored, preset) }
      : null,
    plan: plan ? copy(plan.summary) : null,
    run: run ? { state: run.state, progress: copy(run.progress) } : null,
    counts: countsOf(),
    notices: [...notices],
  });

  const makePlan = (): MockPlan => {
    const writable = writableFormats(tools);
    const jobs: MockJob[] = [];
    const byEntry = new Map<number, MockJob[]>();
    const problems = new Map<string, Tally>();
    const tally = (code: string, text: string) => {
      const item = problems.get(code) ?? { code, text, count: 0 };
      item.count++;
      problems.set(code, item);
    };
    let ready = 0;
    let skipped = 0;
    let blocked = 0;
    const sources = entries.filter((e) => isImage(e) && !e.excluded);
    for (const entry of sources) {
      preset.outputs.forEach((output, target) => {
        const format = output.codec.format;
        const root = roots.find((r) => r.id === entry.root);
        const stem = entry.relativePath.replace(/\.[^.]+$/, "");
        let state: JobState;
        if (!writable.has(format)) {
          state = { state: "blocked", reason: message("no-route", "no installed tool can write this format") };
          tally("no-route", "blocked: no installed tool can write this format");
          blocked++;
        } else if (written.has(`${entry.id}:${target}`)) {
          state = { state: "skipped", reason: message("output-exists", "the output already exists") };
          tally("output-exists", "skipped: the output already exists");
          skipped++;
        } else {
          state = { state: "ready" };
          ready++;
        }
        const job: MockJob = {
          id: jobs.length,
          entry: entry.id,
          target,
          format,
          output: `${root?.isFolder ? root.path : ""}\\${stem}.${EXTENSION_OF[format]}`,
          state,
        };
        jobs.push(job);
        byEntry.set(entry.id, [...(byEntry.get(entry.id) ?? []), job]);
      });
    }
    return {
      jobs,
      byEntry,
      used: false,
      summary: {
        jobs: jobs.length,
        ready,
        skipped,
        blocked,
        sources: sources.length,
        outputsPerSource: preset.outputs.length,
        previousOutputs: 0,
        filteredOut: 0,
        unsupported: entries.length - entries.filter(isImage).length,
        problems: [...problems.values()].sort((a, b) => b.count - a.count),
        notes: [],
        examples: preset.outputs.flatMap((_, target) => {
          const first = jobs.find((j) => j.target === target && j.state.state === "ready") ?? jobs.find((j) => j.target === target);
          const entry = first && entries.find((e) => e.id === first.entry);
          return first && entry ? [{ target, entry: entry.id, source: entry.relativePath, output: first.output }] : [];
        }),
      },
    };
  };

  const finishRun = (current: MockRun, updates: JobUpdate[]) => {
    clearInterval(current.timer);
    for (const job of plan?.jobs ?? []) {
      if (job.state.state === "ready" || job.state.state === "running") {
        job.state = { state: "cancelled" };
        updates.push({ id: job.id, state: job.state });
      }
    }
    current.progress.cancelled = (plan?.jobs ?? []).filter((j) => j.state.state === "cancelled").length;
    current.state = "finished";
    current.progress.running = 0;
    current.progress.remainingMs = null;
    if (updates.length > 0) emit({ type: "jobsChanged", jobs: copy(updates) });
    emit({ type: "runFinished", progress: copy(current.progress) });
  };

  const step = (current: MockRun) => {
    if (current.state === "paused" || !plan) return;
    const { progress } = current;
    progress.elapsedMs += runStepMs;
    const updates: JobUpdate[] = [];
    // The jobs started last step finish now.
    for (const job of plan.jobs.filter((j) => j.state.state === "running")) {
      const entry = entries.find((e) => e.id === job.entry);
      const input = entry?.size ?? 1_000_000;
      if (failEvery > 0 && (job.id + 1) % failEvery === 0) {
        job.state = { state: "failed", message: `${job.format === "webp" ? "cwebp" : "the encoder"} stopped with an error` };
        progress.failed++;
      } else {
        const output = Math.round(input * (job.format === "jxl" ? 0.13 : 0.2));
        job.state = { state: "converted", inputBytes: input, outputBytes: output, seconds: 0.4 + (job.id % 5) / 10 };
        written.add(`${job.entry}:${job.target}`);
        progress.converted++;
        progress.inputBytes += input;
        progress.outputBytes += output;
      }
      updates.push({ id: job.id, state: copy(job.state) });
    }
    if (current.state === "cancelling") {
      finishRun(current, updates);
      return;
    }
    const batch = Math.max(1, Math.ceil(progress.total / 25));
    const next = plan.jobs.filter((j) => j.state.state === "ready").slice(0, batch);
    for (const job of next) {
      job.state = { state: "running" };
      updates.push({ id: job.id, state: { state: "running" } });
    }
    progress.running = next.length;
    const finished = progress.converted + progress.failed;
    progress.remainingMs =
      finished >= 5 ? Math.round((progress.elapsedMs / finished) * (progress.total - finished)) : null;
    if (updates.length > 0) emit({ type: "jobsChanged", jobs: copy(updates) });
    emit({ type: "progress", progress: copy(progress) });
    if (next.length === 0) finishRun(current, []);
  };

  const presetId = (name: string): string => {
    const base = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "preset";
    let id = base;
    for (let n = 2; presets.has(id); n++) id = `${base}-${n}`;
    return id;
  };

  const job = (mock: MockJob): Job => {
    const entry = entries.find((e) => e.id === mock.entry);
    const tool = mock.format === "jxl" ? "cjxl" : mock.format === "webp" ? "cwebp" : "vips";
    return {
      id: mock.id,
      entry: mock.entry,
      source: entry?.relativePath ?? "",
      target: mock.target,
      format: mock.format,
      output: mock.output,
      state: copy(mock.state),
      route: `${tool} writes ${DESCRIPTIONS[mock.format]} directly`,
      tools: [tool],
      notes: [],
      replacesExisting: false,
    };
  };

  return {
    emit,

    overview: async () => overview(),
    dismissNotices: async () => {
      notices = [];
    },

    addInputs: async (paths) => {
      ensureIdle();
      const known = new Set(roots.map((root) => root.path));
      for (const path of paths) if (!known.has(path)) addRoot(path);
      const summary = summarize();
      emit({ type: "scanProgress", discovered: summary.files, inspected: summary.files });
      changed();
      return summarize();
    },
    removeRoot: async (index) => {
      ensureIdle();
      const root = rootAt(index);
      roots = roots.filter((r) => r !== root);
      entries = entries.filter((e) => e.root !== root.id);
      changed();
      return summarize();
    },
    clearInputs: async () => {
      ensureIdle();
      roots = [];
      entries = [];
      changed();
      return summarize();
    },
    tree: async (index, dir) => {
      const root = rootAt(index);
      const prefix = dir ? `${dir}/` : "";
      const folders = new Map<string, FileCounts>();
      const files: TreeNode[] = [];
      for (const entry of entries) {
        if (entry.root !== root.id || !entry.relativePath.startsWith(prefix)) continue;
        const rest = entry.relativePath.slice(prefix.length).split("/");
        const [name = ""] = rest;
        if (rest.length === 1) {
          files.push({ name, relativePath: entry.relativePath, node: { kind: "file", ...fileInfo(entry) } });
        } else {
          const counts = folders.get(name) ?? emptyCounts();
          addTo(counts, entry);
          folders.set(name, counts);
        }
      }
      return [
        ...[...folders].map(([name, counts]): TreeNode => ({
          name,
          relativePath: `${prefix}${name}`,
          node: { kind: "folder", counts },
        })),
        ...files.sort((a, b) => a.name.localeCompare(b.name)),
      ];
    },
    setExcluded: async (index, relativePath, excluded) => {
      ensureIdle();
      const root = rootAt(index);
      for (const entry of entries) {
        const below =
          relativePath === "" ||
          entry.relativePath === relativePath ||
          entry.relativePath.startsWith(`${relativePath}/`);
        if (entry.root === root.id && below) entry.excluded = excluded;
      }
      changed();
      return summarize();
    },
    fileCounts: async () => countsOf(),
    files: async (filter, offset, limit) => {
      const matching = entries.filter((entry) => matches(filter, entry));
      const files: FileRow[] = matching.slice(offset, offset + limit).map((entry) => ({
        root: roots.findIndex((r) => r.id === entry.root),
        name: baseName(entry.relativePath),
        relativePath: entry.relativePath,
        file: fileInfo(entry),
      }));
      return { total: matching.length, offset, files };
    },
    entry: async (id): Promise<EntryDetails> => {
      const entry = entries.find((e) => e.id === id);
      if (!entry) throw failure("notFound", `no file ${id}`);
      const root = roots.find((r) => r.id === entry.root);
      return {
        root: roots.findIndex((r) => r.id === entry.root),
        name: baseName(entry.relativePath),
        relativePath: entry.relativePath,
        path: root?.isFolder ? `${root.path}\\${entry.relativePath.replaceAll("/", "\\")}` : (root?.path ?? ""),
        file: fileInfo(entry),
        bitDepth: null,
        alpha: entry.format === "png" ? null : false,
        frames: null,
        pages: null,
        metadata: entry.format === "jpeg" ? ["EXIF", "ICC profile"] : [],
        extensionMismatch: false,
        jobs: (plan?.byEntry.get(entry.id) ?? []).map(job),
      };
    },
    clearFinished: async () => {
      ensureIdle();
      const done = entries.filter((e) => isImage(e) && !e.excluded && flagsOf(e).done);
      if (done.length === 0) return summarize();
      const gone = new Set(done.map((e) => e.id));
      entries = entries.filter((e) => !gone.has(e.id));
      // An input left with no images goes too.
      roots = roots.filter((root) => entries.some((e) => e.root === root.id && isImage(e)));
      entries = entries.filter((e) => roots.some((r) => r.id === e.root));
      changed();
      return summarize();
    },

    preset: async () => wirePreset(preset),
    setPreset: async (next) => {
      ensureIdle();
      if (!next.name.trim()) throw failure("invalid", "invalid preset: name: must not be empty");
      preset = copy(next);
      changed();
    },
    storedPresets: async (): Promise<StoredPreset[]> => [...presets].map(([id, stored]) => ({ id, preset: wirePreset(stored) })),
    loadPreset: async (id) => {
      const stored = presets.get(id);
      if (!stored) throw failure("notFound", `no preset "${id}"`);
      ensureIdle();
      preset = copy(stored);
      loaded = { id, stored: copy(stored) };
      changed();
      return wirePreset(preset);
    },
    savePreset: async (id, next) => {
      if (next.outputs.length === 0) {
        throw failure("invalid", "invalid preset: outputs: must contain at least one output");
      }
      const key = id ?? presetId(next.name);
      presets.set(key, copy(next));
      if (loaded?.id === key) loaded = { id: key, stored: copy(next) };
      return key;
    },
    deletePreset: async (id) => {
      if (!presets.delete(id)) throw failure("notFound", `no preset ${id}`);
      if (loaded?.id === id) loaded = null;
    },
    newPreset: async () => {
      ensureIdle();
      preset = copy(MOCK_PRESET);
      loaded = null;
      changed();
      return wirePreset(preset);
    },
    saveWorking: async (id, name, description) => {
      if (!name.trim()) throw failure("invalid", "invalid preset: name: must not be empty");
      if (preset.outputs.length === 0) {
        throw failure("invalid", "invalid preset: outputs: must contain at least one output");
      }
      const next: Preset = { ...copy(preset), name, description: description?.trim() ? description : undefined };
      const key = id ?? presetId(name);
      presets.set(key, copy(next));
      preset = next;
      loaded = { id: key, stored: copy(next) };
      return key;
    },
    importPreset: async (path) => {
      const name = baseName(path).replace(/\.json$/i, "").replace(/[-_]+/g, " ");
      const imported: Preset = {
        name: name.charAt(0).toUpperCase() + name.slice(1),
        inputFilter: { mode: "all" },
        outputs: [{ codec: { format: "webp", quality: 80 } }],
      };
      const id = presetId(imported.name);
      presets.set(id, copy(imported));
      return { id, preset: wirePreset(imported) };
    },
    exportPreset: async (id) => {
      if (!presets.has(id)) throw failure("notFound", `no preset "${id}"`);
    },
    codecs: async (): Promise<CodecDefaults[]> =>
      MOCK_FACTORY.map((factory) => {
        const patch: CodecPatch | undefined = (defaults.codecs ?? []).find((c) => c.format === factory.format);
        return { format: factory.format, factory: copy(factory), inherited: { ...copy(factory), ...copy(patch) } as CodecSettings };
      }),

    defaults: async () => wireDefaults(defaults),
    setDefaults: async (next) => {
      ensureIdle();
      defaults = copy(next);
      changed();
    },
    settings: async () => wireSettings(settings),
    setSettings: async (next) => {
      ensureIdle();
      settings = copy(next);
    },
    tools: async () => copy(tools),
    refreshTools: async () => {
      ensureIdle();
      tools = copy(seed.tools ?? MOCK_TOOLS);
      changed();
      return copy(tools);
    },

    plan: async () => {
      ensureIdle();
      if (preset.outputs.length === 0) throw failure("invalid", "pick at least one output format to convert to");
      plan = makePlan();
      run = null;
      return copy(plan.summary);
    },
    jobs: () => notYet("jobs"),
    startRun: async () => {
      ensureIdle();
      if (!plan) throw failure("noPlan", "nothing has been planned yet");
      if (plan.used) throw failure("noPlan", "this plan has already run; plan again to see what's left to do");
      if (plan.summary.ready === 0) throw failure("invalid", "nothing in the plan is ready to convert");
      plan.used = true;
      const current: MockRun = {
        state: "running",
        progress: {
          total: plan.summary.ready,
          converted: 0,
          failed: 0,
          cancelled: 0,
          running: 0,
          inputBytes: 0,
          outputBytes: 0,
          elapsedMs: 0,
          remainingMs: null,
        },
        timer: setInterval(() => step(current), runStepMs),
      };
      run = current;
    },
    pause: async () => {
      if (run?.state !== "running") throw failure("invalid", "nothing is running");
      run.state = "paused";
    },
    resume: async () => {
      if (run?.state !== "paused") throw failure("invalid", "the run isn't paused");
      run.state = "running";
    },
    cancel: async () => {
      if (!run || run.state === "finished") throw failure("invalid", "nothing is running");
      run.state = "cancelling";
    },
    report: async () => {
      if (!plan) throw failure("noPlan", "nothing has been planned yet");
      return `Transmute report\n\n${plan.summary.jobs} job(s)`;
    },
    saveReport: async () => {
      if (!plan) throw failure("noPlan", "nothing has been planned yet");
    },
    runTotals: async (): Promise<TargetTotals[]> => {
      if (!plan) throw failure("noPlan", "nothing has been planned yet");
      return preset.outputs.map((output, target) => {
        const totals: TargetTotals = {
          target,
          format: output.codec.format,
          converted: 0,
          skipped: 0,
          blocked: 0,
          failed: 0,
          cancelled: 0,
          ready: 0,
          inputBytes: 0,
          outputBytes: 0,
        };
        for (const { state } of (plan?.jobs ?? []).filter((j) => j.target === target)) {
          if (state.state === "converted") {
            totals.converted++;
            totals.inputBytes += state.inputBytes;
            totals.outputBytes += state.outputBytes;
          } else if (state.state === "running") totals.ready++;
          else totals[state.state]++;
        }
        return totals;
      });
    },
    reveal: async () => {},

    preview: async (target): Promise<Preview> => {
      if (target.kind === "source") {
        const entry = entries.find((e) => e.id === target.entry);
        if (!entry?.format) throw failure("invalid", "this file isn't an image Transmute can show");
        const { width, height } = { width: entry.width ?? 1600, height: entry.height ?? 900 };
        return {
          path: picture(entry.relativePath, width, height),
          width,
          height,
          bytes: entry.size,
          rendered: false,
          metadata: entry.format === "jpeg" ? ["EXIF", "ICC profile"] : [],
        };
      }
      const job = plan?.jobs.find((j) => j.id === target.job);
      if (job?.state.state !== "converted") throw failure("invalid", "this job hasn't produced an output");
      const entry = entries.find((e) => e.id === job.entry);
      if (!entry) throw failure("unavailable", "the output is no longer there (moved or deleted?)");
      const lossless = (preset.outputs[job.target]?.codec as { lossless?: boolean } | undefined)?.lossless === true;
      return {
        path: picture(entry.relativePath, entry.width ?? 1600, entry.height ?? 900, lossless ? 0 : 0.6),
        width: entry.width,
        height: entry.height,
        bytes: job.state.outputBytes,
        rendered: job.format === "jxl" || job.format === "heif" || job.format === "tiff",
        metadata: entry.format === "jpeg" ? ["EXIF", "ICC profile"] : [],
      };
    },
    sample: async (id, target): Promise<Sample> => {
      const entry = entries.find((e) => e.id === id);
      const output = preset.outputs[target];
      if (!entry?.format) throw failure("notFound", `no file ${id}`);
      if (!output) throw failure("notFound", `no output ${target}`);
      if (!writableFormats(tools).has(output.codec.format)) {
        throw failure("invalid", "no installed tool can write this format");
      }
      const lossless = (output.codec as { lossless?: boolean }).lossless === true;
      const outputBytes = Math.round(entry.size * (lossless ? 0.55 : output.codec.format === "jxl" ? 0.13 : 0.2));
      await new Promise((resolve) => setTimeout(resolve, runStepMs));
      return {
        preview: {
          path: picture(entry.relativePath, entry.width ?? 1600, entry.height ?? 900, lossless ? 0 : 0.6),
          width: entry.width,
          height: entry.height,
          bytes: outputBytes,
          rendered: false,
          metadata: [],
        },
        inputBytes: entry.size,
        outputBytes,
        seconds: 0.4 + (hash(entry.relativePath) % 7) / 10,
        notes: [],
      };
    },
    thumbnail: async (id): Promise<Preview> => {
      const entry = entries.find((e) => e.id === id);
      if (!entry?.format) throw failure("invalid", "this file isn't an image Transmute can show");
      return { path: picture(entry.relativePath, 96, 54), width: entry.width, height: entry.height, bytes: entry.size, rendered: true, metadata: [] };
    },
    clearPreviews: async () => {},
    previewUrl: (preview) => preview.path,

    onEvent: async (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

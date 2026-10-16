import type {
  ApiError,
  ApiEvent,
  AppSettings,
  ErrorCode,
  Format,
  InputsSummary,
  Overview,
  PlanSummary,
  Preset,
  Progress,
  RootInfo,
  RunStatus,
  StoredPreset,
  Tally,
  ToolInfo,
  UserDefaults,
} from "@bindings";
import { writableFormats } from "../lib/formats";
import type { TransmuteApi } from "./client";

/**
 * An in-memory stand-in for `transmute-api`, so the UI runs in a plain
 * browser (`npm run dev`) and in tests without the Tauri shell.
 *
 * It mirrors the real session's rules where the UI depends on them (a plan
 * is invalidated by any edit, a run needs a fresh plan, edits are refused
 * while running) and fakes the rest plausibly. It grows with each GUI
 * phase; commands it doesn't fake yet reject with an `internal` error.
 */
export interface MockApi extends TransmuteApi {
  /** Pushes an event to every listener, as the shell would. */
  emit(event: ApiEvent): void;
}

export interface MockSeed {
  preset?: Preset;
  tools?: ToolInfo[];
  notices?: string[];
  /** Milliseconds between simulated run steps. */
  runStepMs?: number;
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
    status: {
      status: "available",
      path: `C:\\Tools\\${tool}.exe`,
      version,
      source: "path",
      outdated: false,
    },
    reads,
    writes,
    notes: [],
  };
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

/** The mix of formats in a made-up folder. */
const FOLDER_MIX: Array<[Format, number]> = [
  ["jpeg", 0.6],
  ["png", 0.25],
  ["webp", 0.1],
  ["heif", 0.05],
];

interface MockRoot {
  info: RootInfo;
  formats: Map<Format, number>;
  unsupported: number;
}

interface MockPlan {
  summary: PlanSummary;
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

function makeRoot(id: number, path: string): MockRoot {
  const extension = /\.([a-z0-9]+)$/i.exec(path)?.[1]?.toLowerCase();
  if (extension !== undefined) {
    const format = EXTENSIONS[extension];
    return {
      info: { id, path, isFolder: false, files: 1 },
      formats: new Map(format ? [[format, 1]] : []),
      unsupported: format ? 0 : 1,
    };
  }
  const images = 40 + (hash(path) % 400);
  const formats = new Map<Format, number>();
  let counted = 0;
  FOLDER_MIX.forEach(([format, share], index) => {
    const count = index === FOLDER_MIX.length - 1 ? images - counted : Math.round(images * share);
    counted += count;
    formats.set(format, count);
  });
  return { info: { id, path, isFolder: true, files: images + 3 }, formats, unsupported: 3 };
}

function summarize(roots: MockRoot[]): InputsSummary {
  const byFormat = new Map<Format, number>();
  let unsupported = 0;
  for (const root of roots) {
    root.formats.forEach((count, format) => byFormat.set(format, (byFormat.get(format) ?? 0) + count));
    unsupported += root.unsupported;
  }
  const convertible = [...byFormat.values()].reduce((sum, count) => sum + count, 0);
  return {
    roots: roots.map((root) => copy(root.info)),
    files: convertible + unsupported,
    convertible,
    excluded: 0,
    unsupported,
    byFormat: [...byFormat].map(([format, count]) => ({ format, count })),
    warnings: [],
  };
}

export function createMockApi(seed: MockSeed = {}): MockApi {
  const listeners = new Set<(event: ApiEvent) => void>();
  const presets = new Map<string, Preset>();
  const runStepMs = seed.runStepMs ?? 100;
  let preset = copy(seed.preset ?? MOCK_PRESET);
  let tools = copy(seed.tools ?? MOCK_TOOLS);
  let notices = [...(seed.notices ?? [])];
  let defaults: UserDefaults = { codecs: [] };
  let settings: AppSettings = { toolPaths: {} };
  let roots: MockRoot[] = [];
  let nextRoot = 0;
  let plan: MockPlan | null = null;
  let run: MockRun | null = null;
  // Outputs earlier runs wrote, by format: the next plan skips them.
  const written = new Map<Format, number>();

  const emit = (event: ApiEvent) => listeners.forEach((listener) => listener(event));

  const ensureIdle = () => {
    if (run && run.state !== "finished") {
      throw failure("busy", "a conversion is running; wait for it to finish or cancel it");
    }
  };

  /** Any edit invalidates the plan, as in the real session. */
  const changed = () => {
    plan = null;
  };

  const overview = (): Overview => ({
    inputs: summarize(roots),
    preset: copy(preset),
    plan: plan ? copy(plan.summary) : null,
    run: run ? { state: run.state, progress: copy(run.progress) } : null,
    notices: [...notices],
  });

  const makePlan = (): PlanSummary => {
    const sources = summarize(roots).convertible;
    const writable = writableFormats(tools);
    const problems: Tally[] = [];
    let ready = 0;
    let skipped = 0;
    let blocked = 0;
    for (const output of preset.outputs) {
      const format = output.codec.format;
      if (!writable.has(format)) {
        blocked += sources;
        continue;
      }
      const exists = Math.min(written.get(format) ?? 0, sources);
      skipped += exists;
      ready += sources - exists;
    }
    if (skipped > 0) problems.push({ code: "output-exists", text: "The output already exists", count: skipped });
    if (blocked > 0) {
      problems.push({ code: "no-route", text: "No installed tool can write this format", count: blocked });
    }
    return {
      jobs: sources * preset.outputs.length,
      ready,
      skipped,
      blocked,
      sources,
      outputsPerSource: preset.outputs.length,
      previousOutputs: 0,
      filteredOut: 0,
      unsupported: summarize(roots).unsupported,
      problems,
      notes: [],
    };
  };

  const finishRun = (current: MockRun) => {
    clearInterval(current.timer);
    current.state = "finished";
    current.progress.running = 0;
    current.progress.remainingMs = null;
    // Credit the formats written, so re-planning skips them.
    const sources = summarize(roots).convertible;
    for (const output of preset.outputs) {
      const format = output.codec.format;
      if (writableFormats(tools).has(format) && current.progress.cancelled === 0) {
        written.set(format, sources);
      }
    }
    emit({ type: "runFinished", progress: copy(current.progress) });
  };

  const step = (current: MockRun) => {
    if (current.state === "paused") return;
    const { progress } = current;
    progress.elapsedMs += runStepMs;
    if (current.state === "cancelling") {
      progress.cancelled = progress.total - progress.converted - progress.failed;
      finishRun(current);
      return;
    }
    const done = progress.converted + progress.failed;
    const batch = Math.min(Math.max(1, Math.ceil(progress.total / 30)), progress.total - done);
    // One job in fifty fails, so failure counts show up.
    const failures = Math.floor((done + batch) / 50) - Math.floor(done / 50);
    progress.failed += failures;
    progress.converted += batch - failures;
    progress.inputBytes += (batch - failures) * 2_400_000;
    progress.outputBytes += (batch - failures) * 610_000;
    progress.running = Math.min(8, progress.total - progress.converted - progress.failed);
    const finished = progress.converted + progress.failed;
    progress.remainingMs = finished > 0 ? Math.round((progress.elapsedMs / finished) * (progress.total - finished)) : null;
    emit({ type: "progress", progress: copy(progress) });
    if (finished >= progress.total) finishRun(current);
  };

  const presetId = (name: string): string => {
    const base = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "preset";
    let id = base;
    for (let n = 2; presets.has(id); n++) id = `${base}-${n}`;
    return id;
  };

  return {
    emit,

    overview: async () => overview(),
    dismissNotices: async () => {
      notices = [];
    },

    addInputs: async (paths) => {
      ensureIdle();
      const known = new Set(roots.map((root) => root.info.path));
      for (const path of paths) {
        if (!known.has(path)) roots.push(makeRoot(nextRoot++, path));
      }
      const summary = summarize(roots);
      emit({ type: "scanProgress", discovered: summary.files, inspected: summary.files });
      changed();
      return summary;
    },
    removeRoot: async (root) => {
      ensureIdle();
      if (!roots.some((r) => r.info.id === root)) throw failure("notFound", `no input ${root}`);
      roots = roots.filter((r) => r.info.id !== root);
      changed();
      return summarize(roots);
    },
    clearInputs: async () => {
      ensureIdle();
      roots = [];
      changed();
      return summarize(roots);
    },
    tree: () => notYet("tree"),
    setExcluded: () => notYet("setExcluded"),

    preset: async () => copy(preset),
    setPreset: async (next) => {
      ensureIdle();
      if (!next.name.trim()) throw failure("invalid", "invalid preset: name: must not be empty");
      preset = copy(next);
      changed();
    },
    storedPresets: async (): Promise<StoredPreset[]> =>
      [...presets].map(([id, stored]) => ({ id, preset: copy(stored) })),
    loadPreset: async (id) => {
      const stored = presets.get(id);
      if (!stored) throw failure("notFound", `no preset "${id}"`);
      ensureIdle();
      preset = copy(stored);
      changed();
      return copy(preset);
    },
    savePreset: async (id, next) => {
      if (next.outputs.length === 0) {
        throw failure("invalid", "invalid preset: outputs: must contain at least one output");
      }
      const key = id ?? presetId(next.name);
      presets.set(key, copy(next));
      return key;
    },
    deletePreset: async (id) => {
      if (!presets.delete(id)) throw failure("notFound", `no preset ${id}`);
    },

    defaults: async () => copy(defaults),
    setDefaults: async (next) => {
      ensureIdle();
      defaults = copy(next);
      changed();
    },
    settings: async () => copy(settings),
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
      plan = { summary: makePlan(), used: false };
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
    report: () => notYet("report"),

    preview: () => notYet("preview"),
    sample: () => notYet("sample"),
    clearPreviews: async () => {},
    previewUrl: (preview) => preview.path,

    onEvent: async (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

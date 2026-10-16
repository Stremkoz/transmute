import type {
  ApiError,
  ApiEvent,
  AppSettings,
  ErrorCode,
  InputsSummary,
  Overview,
  Preset,
  StoredPreset,
  ToolInfo,
  UserDefaults,
} from "@bindings";
import type { TransmuteApi } from "./client";

/**
 * An in-memory stand-in for `transmute-api`, so the UI runs in a plain
 * browser (`npm run dev`) and in tests without the Tauri shell.
 *
 * It mirrors the real session's rules where the UI depends on them, and
 * grows with each GUI phase. Commands it doesn't fake yet reject with an
 * `internal` error that says so.
 */
export interface MockApi extends TransmuteApi {
  /** Pushes an event to every listener, as the shell would. */
  emit(event: ApiEvent): void;
}

export interface MockSeed {
  preset?: Preset;
  tools?: ToolInfo[];
  notices?: string[];
}

const EMPTY_INPUTS: InputsSummary = {
  roots: [],
  files: 0,
  convertible: 0,
  excluded: 0,
  unsupported: 0,
  byFormat: [],
  warnings: [],
};

export const MOCK_PRESET: Preset = {
  name: "Untitled",
  inputFilter: { mode: "all" },
  outputs: [{ codec: { format: "webp" } }],
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

export function createMockApi(seed: MockSeed = {}): MockApi {
  const listeners = new Set<(event: ApiEvent) => void>();
  const presets = new Map<string, Preset>();
  let preset = copy(seed.preset ?? MOCK_PRESET);
  let tools = copy(seed.tools ?? MOCK_TOOLS);
  let notices = [...(seed.notices ?? [])];
  let defaults: UserDefaults = { codecs: [] };
  let settings: AppSettings = { toolPaths: {} };

  const overview = (): Overview => ({
    inputs: copy(EMPTY_INPUTS),
    preset: copy(preset),
    plan: null,
    run: null,
    notices: [...notices],
  });

  const presetId = (name: string): string => {
    const base = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "preset";
    let id = base;
    for (let n = 2; presets.has(id); n++) id = `${base}-${n}`;
    return id;
  };

  return {
    emit: (event) => listeners.forEach((listener) => listener(event)),

    overview: async () => overview(),
    dismissNotices: async () => {
      notices = [];
    },

    addInputs: () => notYet("addInputs"),
    removeRoot: () => notYet("removeRoot"),
    clearInputs: async () => copy(EMPTY_INPUTS),
    tree: async () => Promise.reject(failure("notFound", "no input 0")),
    setExcluded: async () => Promise.reject(failure("notFound", "no input 0")),

    preset: async () => copy(preset),
    setPreset: async (next) => {
      preset = copy(next);
    },
    storedPresets: async (): Promise<StoredPreset[]> =>
      [...presets].map(([id, stored]) => ({ id, preset: copy(stored) })),
    loadPreset: async (id) => {
      const stored = presets.get(id);
      if (!stored) throw failure("notFound", `no preset ${id}`);
      preset = copy(stored);
      return copy(preset);
    },
    savePreset: async (id, next) => {
      const key = id ?? presetId(next.name);
      presets.set(key, copy(next));
      return key;
    },
    deletePreset: async (id) => {
      if (!presets.delete(id)) throw failure("notFound", `no preset ${id}`);
    },

    defaults: async () => copy(defaults),
    setDefaults: async (next) => {
      defaults = copy(next);
    },
    settings: async () => copy(settings),
    setSettings: async (next) => {
      settings = copy(next);
    },
    tools: async () => copy(tools),
    refreshTools: async () => {
      tools = copy(seed.tools ?? MOCK_TOOLS);
      return copy(tools);
    },

    plan: async () => Promise.reject(failure("invalid", "add some files to convert first")),
    jobs: async () => Promise.reject(failure("noPlan", "nothing has been planned yet")),
    startRun: async () => Promise.reject(failure("noPlan", "nothing has been planned yet")),
    pause: async () => Promise.reject(failure("invalid", "nothing is running")),
    resume: async () => Promise.reject(failure("invalid", "nothing is running")),
    cancel: async () => Promise.reject(failure("invalid", "nothing is running")),
    report: async () => Promise.reject(failure("noPlan", "nothing has run yet")),

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

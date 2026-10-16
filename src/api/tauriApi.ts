import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ApiEvent } from "@bindings";
import type { TransmuteApi } from "./client";

/** The channel the shell emits every `ApiEvent` on (src-tauri/src/main.rs). */
export const EVENT_CHANNEL = "transmute";

/**
 * Shell command names, by client method. They must match the
 * `#[tauri::command]` functions in src-tauri/src/commands.rs; a test
 * compares the two lists.
 */
export const COMMANDS = {
  overview: "overview",
  dismissNotices: "dismiss_notices",
  addInputs: "add_inputs",
  removeRoot: "remove_root",
  clearInputs: "clear_inputs",
  tree: "tree",
  setExcluded: "set_excluded",
  fileCounts: "file_counts",
  files: "files",
  entry: "entry",
  clearFinished: "clear_finished",
  preset: "preset",
  setPreset: "set_preset",
  storedPresets: "stored_presets",
  loadPreset: "load_preset",
  savePreset: "save_preset",
  deletePreset: "delete_preset",
  defaults: "defaults",
  setDefaults: "set_defaults",
  settings: "settings",
  setSettings: "set_settings",
  tools: "tools",
  refreshTools: "refresh_tools",
  plan: "plan",
  jobs: "jobs",
  startRun: "start_run",
  pause: "pause",
  resume: "resume",
  cancel: "cancel",
  report: "report",
  saveReport: "save_report",
  runTotals: "run_totals",
  reveal: "reveal",
  preview: "preview",
  sample: "sample",
  clearPreviews: "clear_previews",
} as const satisfies Partial<Record<keyof TransmuteApi, string>>;

// Tauri renames command arguments to camelCase, so the keys here are the
// Rust parameter names in camelCase.
export const tauriApi: TransmuteApi = {
  overview: () => invoke(COMMANDS.overview),
  dismissNotices: () => invoke(COMMANDS.dismissNotices),

  addInputs: (paths, recursive) => invoke(COMMANDS.addInputs, { paths, recursive }),
  removeRoot: (root) => invoke(COMMANDS.removeRoot, { root }),
  clearInputs: () => invoke(COMMANDS.clearInputs),
  tree: (root, dir) => invoke(COMMANDS.tree, { root, dir }),
  setExcluded: (root, relativePath, excluded) =>
    invoke(COMMANDS.setExcluded, { root, relativePath, excluded }),
  fileCounts: () => invoke(COMMANDS.fileCounts),
  files: (filter, offset, limit) => invoke(COMMANDS.files, { filter, offset, limit }),
  entry: (entry) => invoke(COMMANDS.entry, { entry }),
  clearFinished: () => invoke(COMMANDS.clearFinished),

  preset: () => invoke(COMMANDS.preset),
  setPreset: (preset) => invoke(COMMANDS.setPreset, { preset }),
  storedPresets: () => invoke(COMMANDS.storedPresets),
  loadPreset: (id) => invoke(COMMANDS.loadPreset, { id }),
  savePreset: (id, preset) => invoke(COMMANDS.savePreset, { id, preset }),
  deletePreset: (id) => invoke(COMMANDS.deletePreset, { id }),

  defaults: () => invoke(COMMANDS.defaults),
  setDefaults: (defaults) => invoke(COMMANDS.setDefaults, { defaults }),
  settings: () => invoke(COMMANDS.settings),
  setSettings: (settings) => invoke(COMMANDS.setSettings, { settings }),
  tools: () => invoke(COMMANDS.tools),
  refreshTools: () => invoke(COMMANDS.refreshTools),

  plan: () => invoke(COMMANDS.plan),
  jobs: (filter, offset, limit) => invoke(COMMANDS.jobs, { filter, offset, limit }),
  startRun: () => invoke(COMMANDS.startRun),
  pause: () => invoke(COMMANDS.pause),
  resume: () => invoke(COMMANDS.resume),
  cancel: () => invoke(COMMANDS.cancel),
  report: () => invoke(COMMANDS.report),
  saveReport: (path) => invoke(COMMANDS.saveReport, { path }),
  runTotals: () => invoke(COMMANDS.runTotals),
  reveal: (target) => invoke(COMMANDS.reveal, { target }),

  preview: (target) => invoke(COMMANDS.preview, { target }),
  sample: (entry, target) => invoke(COMMANDS.sample, { entry, target }),
  clearPreviews: () => invoke(COMMANDS.clearPreviews),
  previewUrl: (preview) => convertFileSrc(preview.path),

  onEvent: (listener) => listen<ApiEvent>(EVENT_CHANNEL, (event) => listener(event.payload)),
};

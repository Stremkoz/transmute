import type {
  ApiError,
  ApiEvent,
  AppSettings,
  CodecDefaults,
  EntryDetails,
  FileCounts,
  FileFilter,
  FilePage,
  InputsSummary,
  JobFilter,
  JobPage,
  Overview,
  PlanSummary,
  Preset,
  Preview,
  PreviewTarget,
  Sample,
  StoredPreset,
  TargetTotals,
  ToolInfo,
  TreeNode,
  UserDefaults,
} from "@bindings";

/**
 * The commands of `transmute-api`, one method each (reference: docs/api.md).
 * Two implementations: `tauriApi` (the real backend, through the shell) and
 * `mockApi` (an in-memory fake for the browser and tests).
 *
 * Every method rejects with an {@link ApiError} on failure.
 */
export interface TransmuteApi {
  overview(): Promise<Overview>;
  dismissNotices(): Promise<void>;

  addInputs(paths: string[], recursive: boolean): Promise<InputsSummary>;
  removeRoot(root: number): Promise<InputsSummary>;
  clearInputs(): Promise<InputsSummary>;
  tree(root: number, dir: string): Promise<TreeNode[]>;
  setExcluded(root: number, relativePath: string, excluded: boolean): Promise<InputsSummary>;
  fileCounts(): Promise<FileCounts>;
  files(filter: FileFilter, offset: number, limit: number): Promise<FilePage>;
  entry(entry: number): Promise<EntryDetails>;
  clearFinished(): Promise<InputsSummary>;

  preset(): Promise<Preset>;
  setPreset(preset: Preset): Promise<void>;
  storedPresets(): Promise<StoredPreset[]>;
  loadPreset(id: string): Promise<Preset>;
  savePreset(id: string | null, preset: Preset): Promise<string>;
  /** Deletes a stored preset; if it was loaded, the working copy is now unsaved. */
  deletePreset(id: string): Promise<void>;
  /** Starts over with nothing stored: no outputs, the user's defaults. */
  newPreset(): Promise<Preset>;
  /** Stores the working configuration (over `id`, or as a new preset) and loads it. */
  saveWorking(id: string | null, name: string, description: string | null): Promise<string>;
  importPreset(path: string): Promise<StoredPreset>;
  exportPreset(id: string, path: string): Promise<void>;
  /** Factory and inherited settings for every format. */
  codecs(): Promise<CodecDefaults[]>;

  defaults(): Promise<UserDefaults>;
  setDefaults(defaults: UserDefaults): Promise<void>;
  settings(): Promise<AppSettings>;
  setSettings(settings: AppSettings): Promise<void>;
  tools(): Promise<ToolInfo[]>;
  refreshTools(): Promise<ToolInfo[]>;

  plan(): Promise<PlanSummary>;
  jobs(filter: JobFilter, offset: number, limit: number): Promise<JobPage>;
  startRun(): Promise<void>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  cancel(): Promise<void>;
  report(): Promise<string>;
  saveReport(path: string): Promise<void>;
  runTotals(): Promise<TargetTotals[]>;
  /** Shows a source or a converted output in the file manager. */
  reveal(target: PreviewTarget): Promise<void>;

  preview(target: PreviewTarget): Promise<Preview>;
  sample(entry: number, target: number): Promise<Sample>;
  /** A small cached rendering of one image, for the files list. */
  thumbnail(entry: number): Promise<Preview>;
  clearPreviews(): Promise<void>;
  /** A URL the webview can load for a preview's file. */
  previewUrl(preview: Preview): string;

  /** Subscribes to backend events; resolves to an unsubscribe function. */
  onEvent(listener: (event: ApiEvent) => void): Promise<() => void>;
}

export function isApiError(value: unknown): value is ApiError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as ApiError).code === "string" &&
    typeof (value as ApiError).message === "string"
  );
}

/** A message for any rejection: the API's own text, or a generic fallback. */
export function errorMessage(error: unknown): string {
  if (isApiError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}

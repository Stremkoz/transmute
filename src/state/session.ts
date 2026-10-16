import { createStore } from "zustand/vanilla";
import type {
  ApiEvent,
  Destination,
  FileCounts,
  FileFilter,
  Format,
  Overview,
  Preset,
  PreviewTarget,
  Progress,
  RunState,
  TargetTotals,
  ToolInfo,
  UserDefaults,
} from "@bindings";
import { errorMessage, isApiError, type TransmuteApi } from "../api/client";

/** How long edits settle before the plan is redone (gui-plan: auto-plan). */
export const PLAN_DELAY_MS = 250;
/** How often the files list catches up with a running conversion. */
export const LIST_REFRESH_MS = 750;

/** A run as the workspace shows it, kept after it finishes until an edit. */
export interface RunView {
  state: RunState;
  progress: Progress;
}

const NO_COUNTS: FileCounts = {
  all: 0,
  ready: 0,
  problems: 0,
  excluded: 0,
  running: 0,
  done: 0,
  failed: 0,
  cancelled: 0,
  unsupported: 0,
};

/**
 * What the workspace draws from. The backend owns the truth; this is a
 * cache that commands and events refresh. Every edit re-plans after a short
 * pause, so the bottom bar always describes what Convert would do. After a
 * run, its results stay on screen (no re-plan) until the next edit.
 */
export interface SessionState {
  status: "loading" | "ready" | "failed";
  overview: Overview | null;
  tools: ToolInfo[];
  defaults: UserDefaults;
  /** Why loading failed, when it did. */
  error: string | null;
  /** Counts while inputs are being scanned. */
  scan: { discovered: number; inspected: number } | null;
  planning: boolean;
  planError: string | null;
  run: RunView | null;
  /** Per output, once a run has finished. */
  totals: TargetTotals[] | null;
  counts: FileCounts;
  /** Which files the list shows. */
  filter: FileFilter;
  /** The file whose details are open. */
  selected: number | null;
  /** Bumped whenever the files list's data may have changed. */
  listVersion: number;
  /** The last command that failed, until dismissed. */
  commandError: string | null;

  load(): Promise<void>;
  addInputs(paths: string[]): Promise<void>;
  removeRoot(root: number): Promise<void>;
  clearInputs(): Promise<void>;
  clearFinished(): Promise<void>;
  setExcluded(root: number, relativePath: string, excluded: boolean): Promise<void>;
  addOutput(format: Format): Promise<void>;
  removeOutput(index: number): Promise<void>;
  setDestination(destination: Destination): Promise<void>;
  convert(): Promise<void>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  cancel(): Promise<void>;
  saveReport(path: string): Promise<void>;
  reveal(target: PreviewTarget): Promise<void>;
  setFilter(filter: FileFilter): void;
  select(entry: number | null): void;
  dismissNotices(): Promise<void>;
  dismissError(): void;
  /** Applies one backend event. */
  handle(event: ApiEvent): void;
}

export type SessionStore = ReturnType<typeof createSessionStore>;

/** True while a run is going: the backend refuses edits until it ends. */
export function isRunActive(run: RunView | null): boolean {
  return run !== null && run.state !== "finished";
}

/** Outputs a finished run didn't write that another run could. */
export function leftToConvert(run: RunView | null): number {
  return run?.state === "finished" ? run.progress.failed + run.progress.cancelled : 0;
}

function emptyProgress(total: number): Progress {
  return {
    total,
    converted: 0,
    failed: 0,
    cancelled: 0,
    running: 0,
    inputBytes: 0,
    outputBytes: 0,
    elapsedMs: 0,
    remainingMs: null,
  };
}

function canPlan(overview: Overview): boolean {
  return overview.inputs.convertible > overview.inputs.excluded && overview.preset.outputs.length > 0;
}

export function createSessionStore(api: TransmuteApi) {
  let planTimer: ReturnType<typeof setTimeout> | undefined;
  let listTimer: ReturnType<typeof setTimeout> | undefined;
  // Each scheduled plan gets a number; only the latest may land.
  let planGeneration = 0;

  return createStore<SessionState>()((set, get) => {
    const fail = (error: unknown) => set({ commandError: errorMessage(error) });
    const bumpList = () => set({ listVersion: get().listVersion + 1 });

    /** Redraws from the backend: the overview, its counts and the list. */
    const refresh = async () => {
      try {
        const overview = await api.overview();
        set({ overview, counts: overview.counts, listVersion: get().listVersion + 1 });
      } catch (error) {
        fail(error);
      }
    };

    /** Catches the list up with a run, at most every LIST_REFRESH_MS. */
    const scheduleListRefresh = () => {
      if (listTimer !== undefined) return;
      listTimer = setTimeout(async () => {
        listTimer = undefined;
        try {
          set({ counts: await api.fileCounts() });
        } catch {
          // The next refresh will try again.
        }
        bumpList();
      }, LIST_REFRESH_MS);
    };

    /** Plans, then redraws: the inputs' counts include job states now. */
    const plan = async () => {
      const summary = await api.plan();
      const overview = await api.overview();
      set({ overview, counts: overview.counts });
      bumpList();
      return summary;
    };

    const schedulePlan = () => {
      clearTimeout(planTimer);
      const generation = ++planGeneration;
      const { overview } = get();
      if (!overview || !canPlan(overview)) {
        set({ planning: false, planError: null });
        return;
      }
      set({ planning: true, planError: null });
      planTimer = setTimeout(async () => {
        try {
          await api.plan();
          const overview = await api.overview();
          if (generation !== planGeneration) return;
          set({ planning: false, overview, counts: overview.counts });
          bumpList();
        } catch (error) {
          if (generation !== planGeneration) return;
          // A change landed mid-plan: its own plan is already scheduled.
          if (isApiError(error) && error.code === "noPlan") return;
          set({ planning: false, planError: errorMessage(error) });
        }
      }, PLAN_DELAY_MS);
    };

    /** Runs an edit, then redraws from the backend and re-plans. */
    const edit = async (command: () => Promise<unknown>) => {
      try {
        await command();
        // Any edit ends the review of a finished run, as in the backend.
        set({ run: null, totals: null });
      } catch (error) {
        fail(error);
      }
      await refresh();
      schedulePlan();
    };

    const editPreset = (change: (preset: Preset) => Preset) =>
      edit(async () => {
        const { overview } = get();
        if (overview) await api.setPreset(change(overview.preset));
      });

    const setRunState = (state: RunState) => {
      const { run } = get();
      if (run) set({ run: { ...run, state } });
    };

    const control = async (command: () => Promise<void>, state: RunState) => {
      try {
        await command();
        setRunState(state);
      } catch (error) {
        fail(error);
      }
    };

    const finishRun = async (progress: Progress) => {
      set({ run: { state: "finished", progress } });
      try {
        const [overview, totals] = await Promise.all([api.overview(), api.runTotals()]);
        set({ overview, counts: overview.counts, totals });
      } catch (error) {
        fail(error);
      }
      bumpList();
    };

    return {
      status: "loading",
      overview: null,
      tools: [],
      defaults: { codecs: [] },
      error: null,
      scan: null,
      planning: false,
      planError: null,
      run: null,
      totals: null,
      counts: NO_COUNTS,
      filter: "all",
      selected: null,
      listVersion: 0,
      commandError: null,

      async load() {
        try {
          const [overview, tools, defaults] = await Promise.all([api.overview(), api.tools(), api.defaults()]);
          const run = overview.run && overview.run.state !== "finished" ? overview.run : null;
          set({ status: "ready", overview, counts: overview.counts, tools, defaults, run, error: null });
          if (!run) schedulePlan();
        } catch (error) {
          set({ status: "failed", error: errorMessage(error) });
        }
      },

      async addInputs(paths) {
        set({ scan: { discovered: 0, inspected: 0 } });
        await edit(() => api.addInputs(paths, true));
        set({ scan: null });
      },
      removeRoot: (root) => edit(() => api.removeRoot(root)),
      async clearInputs() {
        set({ filter: "all", selected: null });
        await edit(() => api.clearInputs());
      },
      async clearFinished() {
        set({ filter: "all", selected: null });
        await edit(() => api.clearFinished());
      },
      setExcluded: (root, relativePath, excluded) => edit(() => api.setExcluded(root, relativePath, excluded)),

      addOutput: (format) =>
        editPreset((preset) => ({ ...preset, outputs: [...preset.outputs, { codec: { format } }] })),
      removeOutput: (index) =>
        editPreset((preset) => ({ ...preset, outputs: preset.outputs.filter((_, i) => i !== index) })),
      setDestination: (destination) => editPreset((preset) => ({ ...preset, destination })),

      async convert() {
        const { run, planning } = get();
        if (planning) return;
        try {
          // After a run the plan is spent: plan again for what's left.
          const summary = run?.state === "finished" ? await plan() : get().overview?.plan;
          if (!summary || summary.ready === 0) return;
          // Shown before the call returns: a short run can finish (and send
          // runFinished) before startRun's reply arrives.
          set({ run: { state: "running", progress: emptyProgress(summary.ready) }, totals: null, filter: "all" });
          await api.startRun();
        } catch (error) {
          set({ run: null });
          fail(error);
          // Most often the plan went stale; a fresh one fixes it.
          schedulePlan();
        }
      },
      pause: () => control(api.pause, "paused"),
      resume: () => control(api.resume, "running"),
      cancel: () => control(api.cancel, "cancelling"),

      async saveReport(path) {
        try {
          await api.saveReport(path);
        } catch (error) {
          fail(error);
        }
      },
      async reveal(target) {
        try {
          await api.reveal(target);
        } catch (error) {
          fail(error);
        }
      },

      setFilter: (filter) => set({ filter }),
      select: (selected) => set({ selected }),

      async dismissNotices() {
        await api.dismissNotices();
        const { overview } = get();
        if (overview) set({ overview: { ...overview, notices: [] } });
      },
      dismissError: () => set({ commandError: null }),

      handle(event) {
        switch (event.type) {
          case "scanProgress":
            set({ scan: { discovered: event.discovered, inspected: event.inspected } });
            return;
          case "progress": {
            const { run } = get();
            if (run) set({ run: { ...run, progress: event.progress } });
            return;
          }
          case "jobsChanged":
            scheduleListRefresh();
            return;
          case "runFinished":
            void finishRun(event.progress);
            return;
          case "notice": {
            const { overview } = get();
            if (overview && !overview.notices.includes(event.text)) {
              set({ overview: { ...overview, notices: [...overview.notices, event.text] } });
            }
            return;
          }
        }
      },
    };
  });
}

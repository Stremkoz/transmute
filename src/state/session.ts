import { createStore } from "zustand/vanilla";
import type {
  ApiEvent,
  Destination,
  Format,
  Overview,
  Preset,
  Progress,
  RunState,
  ToolInfo,
  UserDefaults,
} from "@bindings";
import { errorMessage, isApiError, type TransmuteApi } from "../api/client";

/** How long edits settle before the plan is redone (gui-plan: auto-plan). */
export const PLAN_DELAY_MS = 250;

/** A run as the workspace shows it, kept after it finishes until closed. */
export interface RunView {
  state: RunState;
  progress: Progress;
}

/**
 * What the workspace draws from. The backend owns the truth; this is a
 * cache that commands and events refresh. Every edit re-plans after a short
 * pause, so the convert bar always describes what Convert would do.
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
  /** The last command that failed, until dismissed. */
  commandError: string | null;

  load(): Promise<void>;
  addInputs(paths: string[]): Promise<void>;
  removeRoot(root: number): Promise<void>;
  clearInputs(): Promise<void>;
  addOutput(format: Format): Promise<void>;
  removeOutput(index: number): Promise<void>;
  setDestination(destination: Destination): Promise<void>;
  convert(): Promise<void>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  cancel(): Promise<void>;
  /** Leaves a finished run's summary for the inputs again. */
  closeRun(): void;
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
  // Each scheduled plan gets a number; only the latest may land.
  let planGeneration = 0;

  return createStore<SessionState>()((set, get) => {
    const fail = (error: unknown) => set({ commandError: errorMessage(error) });

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
          const plan = await api.plan();
          const current = get().overview;
          if (generation !== planGeneration || !current) return;
          set({ planning: false, overview: { ...current, plan } });
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
      } catch (error) {
        fail(error);
      }
      try {
        set({ overview: await api.overview() });
      } catch (error) {
        fail(error);
      }
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
      commandError: null,

      async load() {
        try {
          const [overview, tools, defaults] = await Promise.all([api.overview(), api.tools(), api.defaults()]);
          const run = overview.run && overview.run.state !== "finished" ? overview.run : null;
          set({ status: "ready", overview, tools, defaults, run, error: null });
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
      clearInputs: () => edit(() => api.clearInputs()),

      addOutput: (format) =>
        editPreset((preset) => ({ ...preset, outputs: [...preset.outputs, { codec: { format } }] })),
      removeOutput: (index) =>
        editPreset((preset) => ({ ...preset, outputs: preset.outputs.filter((_, i) => i !== index) })),
      setDestination: (destination) => editPreset((preset) => ({ ...preset, destination })),

      async convert() {
        const plan = get().overview?.plan;
        if (!plan || get().planning) return;
        // Shown before the call returns: a short run can finish (and send
        // runFinished) before startRun's reply arrives.
        set({ run: { state: "running", progress: emptyProgress(plan.ready) } });
        try {
          await api.startRun();
        } catch (error) {
          set({ run: null });
          fail(error);
          // Most often the plan went stale; a fresh one fixes it.
          schedulePlan();
        }
      },
      async pause() {
        try {
          await api.pause();
          setRunState("paused");
        } catch (error) {
          fail(error);
        }
      },
      async resume() {
        try {
          await api.resume();
          setRunState("running");
        } catch (error) {
          fail(error);
        }
      },
      async cancel() {
        try {
          await api.cancel();
          setRunState("cancelling");
        } catch (error) {
          fail(error);
        }
      },
      closeRun() {
        if (!isRunActive(get().run)) set({ run: null });
      },

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
          case "runFinished":
            set({ run: { state: "finished", progress: event.progress } });
            // The outputs exist now: the next plan shows what's left.
            schedulePlan();
            return;
          case "notice": {
            const { overview } = get();
            if (overview && !overview.notices.includes(event.text)) {
              set({ overview: { ...overview, notices: [...overview.notices, event.text] } });
            }
            return;
          }
          // Per-job changes feed the job list in G2.
          case "jobsChanged":
            return;
        }
      },
    };
  });
}

import { createStore } from "zustand/vanilla";
import type { ApiEvent, Overview, ToolInfo } from "@bindings";
import { errorMessage, type TransmuteApi } from "../api/client";

/**
 * What the workspace draws from: the backend's overview of the session and
 * the installed tools. The backend owns the truth; this is a cache that
 * commands and events refresh.
 */
export interface SessionState {
  status: "loading" | "ready" | "failed";
  overview: Overview | null;
  tools: ToolInfo[];
  /** Why loading failed, when it did. */
  error: string | null;
  load(): Promise<void>;
  dismissNotices(): Promise<void>;
  /** Applies one backend event. */
  handle(event: ApiEvent): void;
}

export type SessionStore = ReturnType<typeof createSessionStore>;

export function createSessionStore(api: TransmuteApi) {
  return createStore<SessionState>()((set, get) => ({
    status: "loading",
    overview: null,
    tools: [],
    error: null,

    async load() {
      try {
        const [overview, tools] = await Promise.all([api.overview(), api.tools()]);
        set({ status: "ready", overview, tools, error: null });
      } catch (error) {
        set({ status: "failed", error: errorMessage(error) });
      }
    },

    async dismissNotices() {
      await api.dismissNotices();
      const { overview } = get();
      if (overview) set({ overview: { ...overview, notices: [] } });
    },

    handle(event) {
      switch (event.type) {
        case "notice": {
          const { overview } = get();
          if (overview && !overview.notices.includes(event.text)) {
            set({ overview: { ...overview, notices: [...overview.notices, event.text] } });
          }
          return;
        }
        // Scans, plans and runs are drawn from G1 on.
        default:
          return;
      }
    },
  }));
}

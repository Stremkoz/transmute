import { isTauri } from "@tauri-apps/api/core";
import type { TransmuteApi } from "./client";

/**
 * The real backend inside the Tauri shell; the mock in a plain browser, so
 * `npm run dev` works for layout and styling without building the app.
 */
export async function connectApi(): Promise<TransmuteApi> {
  if (isTauri()) {
    const { tauriApi } = await import("./tauriApi");
    return tauriApi;
  }
  const { createMockApi } = await import("./mockApi");
  return createMockApi();
}

export * from "./client";

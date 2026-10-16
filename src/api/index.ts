import { isTauri } from "@tauri-apps/api/core";
import type { Platform } from "../platform/platform";
import type { TransmuteApi } from "./client";

export interface Backend {
  api: TransmuteApi;
  platform: Platform;
}

/**
 * The real backend inside the Tauri shell; the mock in a plain browser, so
 * `npm run dev` works for layout and styling without building the app.
 */
export async function connectBackend(): Promise<Backend> {
  if (isTauri()) {
    const [{ tauriApi }, { tauriPlatform }] = await Promise.all([
      import("./tauriApi"),
      import("../platform/tauriPlatform"),
    ]);
    return { api: tauriApi, platform: tauriPlatform };
  }
  const [{ createMockApi }, { createBrowserPlatform }] = await Promise.all([
    import("./mockApi"),
    import("../platform/browserPlatform"),
  ]);
  return { api: createMockApi(), platform: createBrowserPlatform() };
}

export * from "./client";

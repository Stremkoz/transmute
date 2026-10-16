import { render } from "@testing-library/react";
import { App } from "../app/App";
import type { TransmuteApi } from "../api/client";
import { createMockApi, type MockSeed } from "../api/mockApi";
import type { FileDropHandlers, Platform } from "../platform/platform";
import { SessionProvider } from "../state/SessionProvider";

/** A platform whose pickers and drops the test controls. */
export interface TestPlatform extends Platform {
  files: string[];
  folders: string[];
  destination: string | null;
  reportPath: string | null;
  /** Simulates files dropped on the window. */
  drop(paths: string[]): void;
  hover(hovering: boolean): void;
}

export function createTestPlatform(): TestPlatform {
  let handlers: FileDropHandlers | null = null;
  const platform: TestPlatform = {
    files: ["C:\\Pictures\\cat.png"],
    folders: ["C:\\Pictures\\Holiday"],
    destination: "D:\\Converted",
    reportPath: "D:\\report.txt",
    pickFiles: async () => platform.files,
    pickFolders: async () => platform.folders,
    pickDestination: async () => platform.destination,
    pickReportPath: async () => platform.reportPath,
    onFileDrop: async (next) => {
      handlers = next;
      return () => {
        handlers = null;
      };
    },
    // As the shell does: a drop ends the hover.
    drop: (paths) => {
      handlers?.onHover(false);
      handlers?.onDrop(paths);
    },
    hover: (hovering) => handlers?.onHover(hovering),
  };
  return platform;
}

export function renderApp(api: TransmuteApi = createMockApi({ runStepMs: 5 }), platform = createTestPlatform()) {
  const result = render(
    <SessionProvider api={api} platform={platform}>
      <App />
    </SessionProvider>,
  );
  return { ...result, api, platform };
}

export function mockApi(seed: MockSeed = {}) {
  return createMockApi({ runStepMs: 5, ...seed });
}

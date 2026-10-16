import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Platform } from "./platform";

function asList(picked: string | string[] | null): string[] {
  if (picked === null) return [];
  return Array.isArray(picked) ? picked : [picked];
}

// No file-type filter on the pickers: formats are detected from content,
// so a misnamed image is still found, and anything else is reported.
export const tauriPlatform: Platform = {
  pickFiles: async () => asList(await open({ multiple: true, title: "Add images" })),
  pickFolders: async () => asList(await open({ multiple: true, directory: true, title: "Add folders" })),
  pickDestination: async () => {
    const picked = await open({ directory: true, title: "Choose where converted files go" });
    return typeof picked === "string" ? picked : null;
  },
  pickReportPath: () =>
    save({
      title: "Save the conversion report",
      defaultPath: "transmute-report.txt",
      filters: [{ name: "Text", extensions: ["txt"] }],
    }),
  onFileDrop: (handlers) =>
    getCurrentWebview().onDragDropEvent(({ payload }) => {
      switch (payload.type) {
        case "enter":
        case "over":
          handlers.onHover(true);
          return;
        case "leave":
          handlers.onHover(false);
          return;
        case "drop":
          handlers.onHover(false);
          if (payload.paths.length > 0) handlers.onDrop(payload.paths);
          return;
      }
    }),
};

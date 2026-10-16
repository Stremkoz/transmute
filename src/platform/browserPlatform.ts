import type { Platform } from "./platform";

/**
 * Stand-ins for the shell in a plain browser (with the mock API). Pickers
 * return made-up paths the mock backend understands: a path with an
 * extension is a file, anything else a folder of generated images.
 */
export function createBrowserPlatform(): Platform {
  let picks = 0;
  return {
    pickFiles: async () => [`C:\\Users\\Demo\\Pictures\\holiday-${++picks}.jpg`],
    pickFolders: async () => [`C:\\Users\\Demo\\Pictures\\Shoot ${++picks}`],
    pickDestination: async () => "D:\\Converted",
    pickReportPath: async () => "D:\\transmute-report.txt",
    pickPresetFile: async () => "C:\\Users\\Demo\\Downloads\\shared-preset.json",
    pickPresetExportPath: async (name) => `C:\\Users\\Demo\\Documents\\${name}.json`,
    onFileDrop: async (handlers) => {
      const over = (event: DragEvent) => {
        event.preventDefault();
        handlers.onHover(true);
      };
      const leave = (event: DragEvent) => {
        // Leaving a child element isn't leaving the window.
        if (event.relatedTarget === null) handlers.onHover(false);
      };
      const drop = (event: DragEvent) => {
        event.preventDefault();
        handlers.onHover(false);
        // Browsers don't reveal paths; names are enough for the mock.
        const names = [...(event.dataTransfer?.files ?? [])].map((file) => `C:\\Dropped\\${file.name}`);
        if (names.length > 0) handlers.onDrop(names);
      };
      window.addEventListener("dragover", over);
      window.addEventListener("dragleave", leave);
      window.addEventListener("drop", drop);
      return () => {
        window.removeEventListener("dragover", over);
        window.removeEventListener("dragleave", leave);
        window.removeEventListener("drop", drop);
      };
    },
  };
}

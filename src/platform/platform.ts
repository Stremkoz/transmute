import type { Theme } from "@bindings";

/**
 * What the shell offers besides the API: native pickers and file drops.
 * These are the only ways paths enter the app (docs/api.md: the frontend
 * never makes paths up).
 */
export interface Platform {
  /** Files to add; empty when the user cancels. */
  pickFiles(): Promise<string[]>;
  /** Folders to add; empty when the user cancels. */
  pickFolders(): Promise<string[]>;
  /** One folder for converted files to land in; null when cancelled. */
  pickDestination(): Promise<string | null>;
  /** Where to save the run report; null when cancelled. */
  pickReportPath(): Promise<string | null>;
  /** A copy of a tool's program to use instead of the one found; null when cancelled. */
  pickToolPath(tool: string): Promise<string | null>;
  /** A preset file to import; null when cancelled. */
  pickPresetFile(): Promise<string | null>;
  /** Where to export a preset (suggesting `name`); null when cancelled. */
  pickPresetExportPath(name: string): Promise<string | null>;
  /** Follows files dragged over and dropped on the window. */
  onFileDrop(handlers: FileDropHandlers): Promise<() => void>;
  /** Draws the window's frame (title bar) in `theme`; null follows the system. */
  setWindowTheme(theme: Theme | null): Promise<void>;
}

export interface FileDropHandlers {
  /** True while files are dragged over the window. */
  onHover(hovering: boolean): void;
  onDrop(paths: string[]): void;
}

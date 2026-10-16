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
  /** A preset file to import; null when cancelled. */
  pickPresetFile(): Promise<string | null>;
  /** Where to export a preset (suggesting `name`); null when cancelled. */
  pickPresetExportPath(name: string): Promise<string | null>;
  /** Follows files dragged over and dropped on the window. */
  onFileDrop(handlers: FileDropHandlers): Promise<() => void>;
}

export interface FileDropHandlers {
  /** True while files are dragged over the window. */
  onHover(hovering: boolean): void;
  onDrop(paths: string[]): void;
}

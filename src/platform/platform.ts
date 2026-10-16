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
  /** Follows files dragged over and dropped on the window. */
  onFileDrop(handlers: FileDropHandlers): Promise<() => void>;
}

export interface FileDropHandlers {
  /** True while files are dragged over the window. */
  onHover(hovering: boolean): void;
  onDrop(paths: string[]): void;
}

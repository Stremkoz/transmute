import { useEffect, useState } from "react";
import { usePlatform, useSession } from "../state/sessionContext";

const DARK = "(prefers-color-scheme: dark)";

/** The system's preference, kept current (dark where it can't be read). */
function useSystemDark(): boolean {
  const query = typeof window.matchMedia === "function" ? window.matchMedia(DARK) : null;
  const [dark, setDark] = useState(query?.matches ?? true);
  useEffect(() => {
    if (!query) return;
    const change = (event: MediaQueryListEvent) => setDark(event.matches);
    query.addEventListener("change", change);
    return () => query.removeEventListener("change", change);
  }, [query]);
  return dark;
}

/**
 * Applies the chosen theme, or the system's while none is chosen: the
 * colours (tokens.css keys on `data-theme`) and the window's frame.
 */
export function useTheme() {
  const platform = usePlatform();
  const chosen = useSession((state) => state.settings.theme);
  const systemDark = useSystemDark();
  const theme = chosen ?? (systemDark ? "dark" : "light");

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  useEffect(() => {
    // Only the frame: the page's colours already follow the choice.
    platform.setWindowTheme(chosen ?? null).catch((error: unknown) => console.warn("couldn't theme the window frame", error));
  }, [platform, chosen]);
}

import { useEffect, useState, type ReactNode } from "react";
import type { TransmuteApi } from "../api/client";
import type { Platform } from "../platform/platform";
import { createSessionStore } from "./session";
import { SessionContext } from "./sessionContext";

/** Connects the UI to one backend: loads the session and follows its events. */
interface SessionProviderProps {
  api: TransmuteApi;
  platform: Platform;
  children: ReactNode;
}

export function SessionProvider({ api, platform, children }: SessionProviderProps) {
  const [store] = useState(() => createSessionStore(api));

  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    let active = true;
    void api.onEvent((event) => store.getState().handle(event)).then((stop) => {
      if (active) unsubscribe = stop;
      else stop();
    });
    void store.getState().load();
    return () => {
      active = false;
      unsubscribe?.();
    };
  }, [api, store]);

  return <SessionContext.Provider value={{ api, platform, store }}>{children}</SessionContext.Provider>;
}

import { useEffect, useState, type ReactNode } from "react";
import type { TransmuteApi } from "../api/client";
import { createSessionStore } from "./session";
import { SessionContext } from "./sessionContext";

/** Connects the UI to one backend: loads the session and follows its events. */
export function SessionProvider({ api, children }: { api: TransmuteApi; children: ReactNode }) {
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

  return <SessionContext.Provider value={{ api, store }}>{children}</SessionContext.Provider>;
}

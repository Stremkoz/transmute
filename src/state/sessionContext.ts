import { createContext, useContext } from "react";
import { useStore } from "zustand";
import type { TransmuteApi } from "../api/client";
import type { Platform } from "../platform/platform";
import type { SessionState, SessionStore } from "./session";

export interface SessionContextValue {
  api: TransmuteApi;
  platform: Platform;
  store: SessionStore;
}

export const SessionContext = createContext<SessionContextValue | null>(null);

function useSessionContext(): SessionContextValue {
  const value = useContext(SessionContext);
  if (!value) throw new Error("useSession needs a <SessionProvider> above it");
  return value;
}

export function useSession<T>(selector: (state: SessionState) => T): T {
  return useStore(useSessionContext().store, selector);
}

export function useApi(): TransmuteApi {
  return useSessionContext().api;
}

export function usePlatform(): Platform {
  return useSessionContext().platform;
}

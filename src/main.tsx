import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { connectBackend } from "./api";
import { App } from "./app/App";
import { ErrorBoundary } from "./app/ErrorBoundary";
import { SessionProvider } from "./state/SessionProvider";
import "./styles/global.css";

const container = document.getElementById("root");
if (!container) throw new Error("index.html has no #root element");

const { api, platform } = await connectBackend();
createRoot(container).render(
  <StrictMode>
    <ErrorBoundary>
      <SessionProvider api={api} platform={platform}>
        <App />
      </SessionProvider>
    </ErrorBoundary>
  </StrictMode>,
);

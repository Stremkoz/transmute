import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { connectApi } from "./api";
import { App } from "./app/App";
import { SessionProvider } from "./state/SessionProvider";
import "./styles/global.css";

const container = document.getElementById("root");
if (!container) throw new Error("index.html has no #root element");

const api = await connectApi();
createRoot(container).render(
  <StrictMode>
    <SessionProvider api={api}>
      <App />
    </SessionProvider>
  </StrictMode>,
);

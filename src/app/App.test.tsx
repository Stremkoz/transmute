import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { TransmuteApi } from "../api/client";
import { createMockApi } from "../api/mockApi";
import { SessionProvider } from "../state/SessionProvider";
import { App } from "./App";

function renderApp(api: TransmuteApi) {
  return render(
    <SessionProvider api={api}>
      <App />
    </SessionProvider>,
  );
}

describe("App", () => {
  it("shows the empty workspace from the session overview", async () => {
    renderApp(createMockApi());

    expect(await screen.findByText("Drop images or folders here")).toBeInTheDocument();
    const outputs = screen.getByRole("region", { name: "Outputs" });
    expect(within(outputs).getByText("WebP")).toBeInTheDocument();
    expect(within(outputs).getByText("Beside each source")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Convert" })).toBeDisabled();
    expect(screen.getByText("Add images to get started.")).toBeInTheDocument();
  });

  it("says when there are no outputs to convert to", async () => {
    renderApp(createMockApi({ preset: { name: "Empty", inputFilter: { mode: "all" }, outputs: [] } }));

    expect(await screen.findByText("Pick at least one format to convert to.")).toBeInTheDocument();
  });

  it("lists the tools, with missing ones flagged", async () => {
    const user = userEvent.setup();
    renderApp(createMockApi());

    await user.click(await screen.findByRole("button", { name: /Tools.*2 missing/ }));

    const dialog = screen.getByRole("dialog", { name: "Tools" });
    const ffmpeg = within(dialog).getByRole("row", { name: /^FFmpeg/ });
    expect(within(ffmpeg).getByText("Not found")).toBeInTheDocument();
    const cjxl = within(dialog).getByRole("row", { name: /JPEG XL encoder/ });
    expect(within(cjxl).getByText("0.11.1")).toBeInTheDocument();
  });

  it("shows notices from the overview and from events, and dismisses them", async () => {
    const user = userEvent.setup();
    const api = createMockApi({ notices: ["settings.json was damaged and has been backed up"] });
    renderApp(api);

    expect(await screen.findByText("settings.json was damaged and has been backed up")).toBeInTheDocument();
    act(() => api.emit({ type: "notice", text: "a run couldn't start" }));
    expect(screen.getByText("a run couldn't start")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect((await api.overview()).notices).toEqual([]);
  });

  it("reports a session that fails to load", async () => {
    const api = createMockApi();
    api.overview = () => Promise.reject({ code: "store", message: "can't tell where to keep settings" });
    renderApp(api);

    expect(await screen.findByRole("alert")).toHaveTextContent("can't tell where to keep settings");
  });
});
